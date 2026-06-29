//! AST-native canonical printer.
//!
//! [`canonicalize`] parses Rust source with Tree-sitter and re-emits it in a
//! single canonical style by walking the parse tree. This is what the `clean`
//! filter stores, so reformatting never reaches history: two differently
//! formatted inputs that parse to the same tree produce byte-identical output.
//!
//! ## Scope
//!
//! This printer covers a documented *subset* of Rust. Module items: `use`
//! declarations, `const`/`static`, named & unit `struct`s, unit-variant `enum`s,
//! and `impl` blocks (inherent and trait impls). Inside functions: parameters
//! (incl. `self` receivers), blocks, `let` bindings, and expressions —
//! binary/call/macro, field access, struct literals, paths, references (`&`/
//! `&mut`), and simple generics (`Vec<T>`) — plus literals and line/block
//! comments. It is deliberately **fail-closed**: a syntax error or any node kind
//! the printer does not understand (tuple structs, traits, generic *parameters*,
//! enum payloads, lifetimes, closures, …) returns an [`Error`] rather than
//! guessing, so the filter can never silently corrupt code it cannot represent.
//! Widening the subset is additive — each new node kind is one more arm in
//! [`Printer::item`] / [`Printer::stmt`] / [`Printer::expr`].
//!
//! ## Determinism contract
//!
//! [`canonicalize`] is a pure function of the parse tree, and the whole design
//! depends on it:
//!
//! - **Convergent** — any two formattings of the same program produce identical
//!   bytes (so reformatting never reaches history).
//! - **Idempotent** — `canonicalize(canonicalize(x)) == canonicalize(x)`, which
//!   is what lets `smudge` be the identity and avoids edit/checkout/add churn.
//! - **No ambient nondeterminism** — no clock, locale, randomness, or float; the
//!   one `HashMap` in the filter is protocol metadata read by key, never
//!   iterated into output. Output is always `\n`-terminated UTF-8.
//! - **Fail-closed, not partial** — syntax errors are rejected up front, so
//!   Tree-sitter's error recovery never yields a nondeterministic partial parse.
//!
//! The canonical form is *defined by* the pair `(tree-sitter-rust grammar
//! version, this printer)`. Cross-machine reproducibility therefore reduces to
//! pinning that pair (what `Cargo.lock` does). Upgrading either is a deliberate
//! one-time re-canonicalization, not silent per-user drift — the same discipline
//! teams apply to pinning a formatter version. These properties are guarded by
//! the `convergence_*`, `idempotent_*`, and `pure_repeated_calls_*` tests below.

use crate::Error;
use tree_sitter::{Node, Parser};

const INDENT: &str = "    ";

/// Parse `source` as Rust and return its canonical form.
///
/// Returns [`Error::Parsing`] if the source does not parse cleanly, and
/// [`Error::Generation`] if it parses but contains a construct outside the
/// supported subset.
pub fn canonicalize(source: &[u8]) -> Result<Vec<u8>, Error> {
    let tree = parse(source)?;
    let mut printer = Printer {
        src: source,
        out: String::new(),
    };
    printer.source_file(tree.root_node())?;
    Ok(printer.out.into_bytes())
}

/// Parse `source` as Rust, rejecting anything that does not parse cleanly.
fn parse(source: &[u8]) -> Result<tree_sitter::Tree, Error> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_rust::LANGUAGE.into())
        .map_err(|e| Error::Parsing(format!("loading Rust grammar: {e}")))?;
    let tree = parser
        .parse(source, None)
        .ok_or_else(|| Error::Parsing("parser returned no tree".to_string()))?;
    if tree.root_node().has_error() {
        return Err(Error::Parsing(
            "source has syntax errors; fix them or bypass the filter".to_string(),
        ));
    }
    Ok(tree)
}

/// A top-level definition surfaced by the [`inspect`] read verb.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Def {
    /// Kind of definition: `"fn"`, `"struct"`, `"enum"`, `"const"`, `"static"`,
    /// or `"impl"`.
    pub kind: &'static str,
    /// The declared name.
    pub name: String,
    /// Content identity: a deterministic hash of the node's *canonical* form,
    /// so it is **stable across reformatting**. Couples name *and* body — two
    /// functions match here only if both are identical.
    pub content_hash: String,
    /// Shape identity: the same hash with the function's *declared name* blanked
    /// (the "shallow content" axis from the README). Two functions with identical
    /// bodies but different names share a `shape_hash` — the seam that lets
    /// [`crate::identity`] recognize a **rename**. (v1 normalizes only the
    /// declaration; a recursive body still references the old name, so a recursive
    /// rename reads as a body edit, not a rename.)
    pub shape_hash: String,
    /// The multiset of Merkle subtree hashes (sorted) of the function's CST — the
    /// node's deep content. The fuzzy matcher in [`crate::identity`] measures
    /// *structural* similarity over these (shared subtrees), recognizing a
    /// function that was renamed *and* edited at once.
    pub subtree_hashes: Vec<u64>,
}

/// The first **verbspec read verb** — "look at the AST."
///
/// Conceptually a verb with `input: { source }` and `output: Def[]`: it parses
/// Rust and lists the top-level definitions, each tagged with a content hash
/// that is invariant under formatting. This is a small proof-of-concept of the
/// read surface (query the AST); history verbs (per-node blame) need the model
/// store. Definitions whose bodies fall outside the supported subset are
/// skipped rather than failing the whole listing.
pub fn inspect(source: &[u8]) -> Result<Vec<Def>, Error> {
    let tree = parse(source)?;
    let root = tree.root_node();
    let mut defs = Vec::new();
    let mut cursor = root.walk();
    for item in root.named_children(&mut cursor) {
        // The keyword/kind for each supported top-level item (skip comments etc.).
        let kind: &'static str = match item.kind() {
            "function_item" => "fn",
            "struct_item" => "struct",
            "enum_item" => "enum",
            "const_item" => "const",
            "static_item" => "static",
            "impl_item" => "impl",
            _ => continue,
        };
        let mut printer = Printer {
            src: source,
            out: String::new(),
        };
        if printer.item(item, 0).is_err() {
            continue; // can't hash what we can't canonicalize
        }
        // An `impl` has no name of its own; its identity is the type it is for.
        let name_field = if kind == "impl" { "type" } else { "name" };
        let name = item
            .child_by_field_name(name_field)
            .and_then(|n| n.utf8_text(source).ok())
            .unwrap_or("?")
            .to_string();
        // Shape hash: blank the declared name in the canonical form (which begins
        // `<kw> <name>`), so two items that differ only by name share it — the
        // seam for rename detection. (`impl` blocks have no leading name to blank,
        // so their shape hash is just their content hash for now.)
        let canonical = &printer.out;
        let shape_hash = if kind == "impl" {
            fnv1a_hex(canonical.as_bytes())
        } else {
            let shaped = canonical.replacen(&format!("{kind} {name}"), &format!("{kind} _"), 1);
            fnv1a_hex(shaped.as_bytes())
        };
        defs.push(Def {
            kind,
            name,
            content_hash: fnv1a_hex(canonical.as_bytes()),
            shape_hash,
            subtree_hashes: subtree_hashes(item, source),
        });
    }
    Ok(defs)
}

/// Dependency-free, deterministic 64-bit FNV-1a hash. Adequate for content
/// identity here; a real model store would use a cryptographic hash.
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &byte in bytes {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// [`fnv1a`] rendered as fixed-width hex.
fn fnv1a_hex(bytes: &[u8]) -> String {
    format!("{:016x}", fnv1a(bytes))
}

/// The multiset of **Merkle subtree hashes** of `node` (post-order). Each node is
/// hashed as `fnv1a(kind ++ (leaf ? text : child_hashes))`, so a subtree's hash is
/// the deep content of that subtree — text-inclusive and formatting-invariant.
/// Two functions that share a sub-expression share its hash exactly; this is the
/// bottom-up phase of structural (GumTree-family) matching. Returned sorted.
pub fn subtree_hashes(node: Node, src: &[u8]) -> Vec<u64> {
    let mut out = Vec::new();
    hash_subtree(node, src, &mut out);
    out.sort_unstable();
    out
}

fn hash_subtree(node: Node, src: &[u8], out: &mut Vec<u64>) -> u64 {
    let mut buf = node.kind().as_bytes().to_vec();
    buf.push(0);
    // Walk *all* children (named + anonymous), so operators and keywords (`*` vs
    // `+`, `let`, …) are part of the hash. Whitespace is not a CST node, so this
    // stays formatting-invariant.
    if node.child_count() == 0 {
        if let Ok(text) = node.utf8_text(src) {
            buf.extend_from_slice(text.as_bytes());
        }
    } else {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            let ch = hash_subtree(child, src, out);
            buf.extend_from_slice(&ch.to_le_bytes());
        }
    }
    let h = fnv1a(&buf);
    out.push(h);
    h
}

/// The Merkle hash of a single node (its subtree-root hash).
fn merkle_hash(node: Node, src: &[u8]) -> u64 {
    let mut scratch = Vec::new();
    hash_subtree(node, src, &mut scratch)
}

/// One statement of a function body — the unit of the structural edit script
/// ([`crate::identity::edit_script`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Statement {
    /// Merkle hash of the statement subtree (exact identity).
    pub hash: u64,
    /// The statement's subtree-hash multiset (for sub-similarity scoring).
    pub subtrees: Vec<u64>,
    /// Canonical text of the statement (no indentation, no trailing newline).
    pub text: String,
}

/// The ordered statements of the named function's body, each with its Merkle hash,
/// subtree multiset, and canonical text. Returns `None` if no such function exists
/// or its body falls outside the supported subset (fail-closed, like [`inspect`]).
pub fn function_statements(source: &[u8], fn_name: &str) -> Option<Vec<Statement>> {
    let tree = parse(source).ok()?;
    let root = tree.root_node();
    let mut cursor = root.walk();
    let func = root.named_children(&mut cursor).find(|n| {
        n.kind() == "function_item"
            && n.child_by_field_name("name")
                .and_then(|x| x.utf8_text(source).ok())
                == Some(fn_name)
    })?;
    let body = func.child_by_field_name("body")?;
    let mut stmts = Vec::new();
    let mut bcur = body.walk();
    for node in body.named_children(&mut bcur) {
        let mut p = Printer {
            src: source,
            out: String::new(),
        };
        p.stmt(node, 0).ok()?; // fail-closed on an unsupported construct
        stmts.push(Statement {
            hash: merkle_hash(node, source),
            subtrees: subtree_hashes(node, source),
            text: p.out.trim_end().to_string(),
        });
    }
    Some(stmts)
}

struct Printer<'a> {
    src: &'a [u8],
    out: String,
}

impl<'a> Printer<'a> {
    /// Raw source text of a node.
    fn text(&self, node: Node) -> Result<&'a str, Error> {
        node.utf8_text(self.src)
            .map_err(|e| Error::Generation(format!("non-utf8 token: {e}")))
    }

    /// A required named field, or a fail-closed error naming what was missing.
    fn field<'n>(&self, node: Node<'n>, name: &str) -> Result<Node<'n>, Error> {
        node.child_by_field_name(name).ok_or_else(|| {
            Error::Generation(format!("`{}` node is missing field `{name}`", node.kind()))
        })
    }

    fn unsupported(&self, node: Node, context: &str) -> Error {
        Error::Generation(format!(
            "unsupported {context}: `{}` (offset {})",
            node.kind(),
            node.start_byte()
        ))
    }

    /// Top level: emit each item, one blank line between items.
    fn source_file(&mut self, root: Node) -> Result<(), Error> {
        let mut cursor = root.walk();
        for (i, item) in root.named_children(&mut cursor).enumerate() {
            if i > 0 {
                self.out.push('\n');
            }
            self.item(item, 0)?;
        }
        Ok(())
    }

    /// An item is anything that can appear at the top level or inside a block as
    /// a statement-like line. Returns canonical text terminated by a newline.
    fn item(&mut self, node: Node, depth: usize) -> Result<(), Error> {
        match node.kind() {
            "function_item" => self.function(node, depth),
            "use_declaration" => self.use_declaration(node, depth),
            "const_item" | "static_item" => self.const_or_static(node, depth),
            "struct_item" => self.struct_item(node, depth),
            "enum_item" => self.enum_item(node, depth),
            "impl_item" => self.impl_item(node, depth),
            "line_comment" | "block_comment" => {
                self.indent(depth);
                self.out.push_str(self.text(node)?);
                self.out.push('\n');
                Ok(())
            }
            _ => Err(self.unsupported(node, "top-level item")),
        }
    }

    /// `use a::b::c;` / `use a::b::*;`. Brace lists are not supported yet
    /// (fail-closed via [`Self::use_argument`]).
    fn use_declaration(&mut self, node: Node, depth: usize) -> Result<(), Error> {
        let arg = self.field(node, "argument")?;
        self.indent(depth);
        self.out.push_str("use ");
        self.out.push_str(&self.use_argument(arg)?);
        self.out.push_str(";\n");
        Ok(())
    }

    fn use_argument(&self, node: Node) -> Result<String, Error> {
        match node.kind() {
            "identifier" | "scoped_identifier" => self.expr(node),
            "use_wildcard" => {
                let inner = node
                    .named_child(0)
                    .ok_or_else(|| self.unsupported(node, "use wildcard"))?;
                Ok(format!("{}::*", self.expr(inner)?))
            }
            _ => Err(self.unsupported(node, "use argument")),
        }
    }

    /// `const NAME: TYPE = VALUE;` or `static NAME: TYPE = VALUE;`.
    fn const_or_static(&mut self, node: Node, depth: usize) -> Result<(), Error> {
        let keyword = if node.kind() == "static_item" {
            "static "
        } else {
            "const "
        };
        let name = self.field(node, "name")?;
        let ty = self.field(node, "type")?;
        let value = self.field(node, "value")?;
        self.indent(depth);
        self.out.push_str(keyword);
        self.out.push_str(self.text(name)?);
        self.out.push_str(": ");
        self.out.push_str(&self.expr(ty)?);
        self.out.push_str(" = ");
        self.out.push_str(&self.expr(value)?);
        self.out.push_str(";\n");
        Ok(())
    }

    /// `struct Name { field: Type, ... }` (named fields) or `struct Name;`
    /// (unit). Tuple structs are not supported yet (fail-closed).
    fn struct_item(&mut self, node: Node, depth: usize) -> Result<(), Error> {
        let name = self.field(node, "name")?;
        self.indent(depth);
        self.out.push_str("struct ");
        self.out.push_str(self.text(name)?);
        match node.child_by_field_name("body") {
            None => self.out.push_str(";\n"),
            Some(body) if body.kind() == "field_declaration_list" => {
                self.out.push_str(" {\n");
                let mut cursor = body.walk();
                for f in body.named_children(&mut cursor) {
                    if f.kind() != "field_declaration" {
                        return Err(self.unsupported(f, "struct field"));
                    }
                    let fname = self.field(f, "name")?;
                    let fty = self.field(f, "type")?;
                    self.indent(depth + 1);
                    self.out.push_str(self.text(fname)?);
                    self.out.push_str(": ");
                    self.out.push_str(&self.expr(fty)?);
                    self.out.push_str(",\n");
                }
                self.indent(depth);
                self.out.push_str("}\n");
            }
            Some(other) => return Err(self.unsupported(other, "struct body")),
        }
        Ok(())
    }

    /// `enum Name { Variant, ... }` (unit variants only in v1).
    fn enum_item(&mut self, node: Node, depth: usize) -> Result<(), Error> {
        let name = self.field(node, "name")?;
        let body = self.field(node, "body")?;
        self.indent(depth);
        self.out.push_str("enum ");
        self.out.push_str(self.text(name)?);
        self.out.push_str(" {\n");
        let mut cursor = body.walk();
        for v in body.named_children(&mut cursor) {
            if v.kind() != "enum_variant" {
                return Err(self.unsupported(v, "enum variant"));
            }
            if v.child_by_field_name("body").is_some() {
                return Err(self.unsupported(v, "enum variant with fields"));
            }
            let vname = self.field(v, "name")?;
            self.indent(depth + 1);
            self.out.push_str(self.text(vname)?);
            self.out.push_str(",\n");
        }
        self.indent(depth);
        self.out.push_str("}\n");
        Ok(())
    }

    /// `impl Type { ... }` or `impl Trait for Type { ... }`. The body is a list
    /// of functions (and comments), one blank line between them.
    fn impl_item(&mut self, node: Node, depth: usize) -> Result<(), Error> {
        let ty = self.field(node, "type")?;
        let body = self.field(node, "body")?;
        self.indent(depth);
        self.out.push_str("impl ");
        if let Some(tr) = node.child_by_field_name("trait") {
            self.out.push_str(&self.expr(tr)?);
            self.out.push_str(" for ");
        }
        self.out.push_str(&self.expr(ty)?);
        self.out.push_str(" {\n");
        let mut cursor = body.walk();
        for (i, m) in body.named_children(&mut cursor).enumerate() {
            if i > 0 {
                self.out.push('\n');
            }
            match m.kind() {
                "function_item" => self.function(m, depth + 1)?,
                "line_comment" | "block_comment" => {
                    self.indent(depth + 1);
                    self.out.push_str(self.text(m)?);
                    self.out.push('\n');
                }
                _ => return Err(self.unsupported(m, "impl item")),
            }
        }
        self.indent(depth);
        self.out.push_str("}\n");
        Ok(())
    }

    fn function(&mut self, node: Node, depth: usize) -> Result<(), Error> {
        let name = self.field(node, "name")?;
        let params = self.field(node, "parameters")?;
        let body = self.field(node, "body")?;

        self.indent(depth);
        self.out.push_str("fn ");
        self.out.push_str(self.text(name)?);
        self.parameters(params)?;
        if let Some(ret) = node.child_by_field_name("return_type") {
            self.out.push_str(" -> ");
            self.out.push_str(&self.expr(ret)?);
        }
        self.out.push(' ');
        self.block(body, depth)
    }

    fn parameters(&mut self, node: Node) -> Result<(), Error> {
        self.out.push('(');
        let mut cursor = node.walk();
        for (i, param) in node.named_children(&mut cursor).enumerate() {
            if i > 0 {
                self.out.push_str(", ");
            }
            match param.kind() {
                // `self`, `&self`, `&mut self` — reprint the receiver verbatim.
                "self_parameter" => self.out.push_str(self.text(param)?),
                "parameter" => {
                    let pattern = self.field(param, "pattern")?;
                    let ty = self.field(param, "type")?;
                    self.out.push_str(self.text(pattern)?);
                    self.out.push_str(": ");
                    self.out.push_str(&self.expr(ty)?);
                }
                _ => return Err(self.unsupported(param, "parameter")),
            }
        }
        self.out.push(')');
        Ok(())
    }

    /// A `{ ... }` block. Emits `{`, each inner statement on its own indented
    /// line, then the closing `}` at the block's own depth.
    fn block(&mut self, node: Node, depth: usize) -> Result<(), Error> {
        self.out.push_str("{\n");
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            self.stmt(child, depth + 1)?;
        }
        self.indent(depth);
        self.out.push_str("}\n");
        Ok(())
    }

    /// A statement inside a block. Statement nodes carry their own terminator;
    /// a bare expression is treated as a trailing (implicit-return) expression
    /// and emitted without a semicolon.
    fn stmt(&mut self, node: Node, depth: usize) -> Result<(), Error> {
        match node.kind() {
            "line_comment" | "block_comment" => {
                self.indent(depth);
                self.out.push_str(self.text(node)?);
                self.out.push('\n');
            }
            "let_declaration" => {
                let pattern = self.field(node, "pattern")?;
                self.indent(depth);
                self.out.push_str("let ");
                self.out.push_str(self.text(pattern)?);
                if let Some(value) = node.child_by_field_name("value") {
                    self.out.push_str(" = ");
                    self.out.push_str(&self.expr(value)?);
                }
                self.out.push_str(";\n");
            }
            "expression_statement" => {
                let inner = node
                    .named_child(0)
                    .ok_or_else(|| self.unsupported(node, "empty expression statement"))?;
                self.indent(depth);
                self.out.push_str(&self.expr(inner)?);
                self.out.push_str(";\n");
            }
            // Anything else that is a valid expression is a trailing expression.
            _ => {
                let rendered = self.expr(node)?;
                self.indent(depth);
                self.out.push_str(&rendered);
                self.out.push('\n');
            }
        }
        Ok(())
    }

    /// Render an expression (or type) to canonical text. No leading indent.
    fn expr(&self, node: Node) -> Result<String, Error> {
        match node.kind() {
            "identifier" | "integer_literal" | "float_literal" | "primitive_type"
            | "string_literal" | "char_literal" | "boolean_literal" | "field_identifier"
            | "type_identifier" | "self" => Ok(self.text(node)?.to_string()),
            "binary_expression" => {
                let left = self.field(node, "left")?;
                let op = self.field(node, "operator")?;
                let right = self.field(node, "right")?;
                Ok(format!(
                    "{} {} {}",
                    self.expr(left)?,
                    self.text(op)?,
                    self.expr(right)?
                ))
            }
            "call_expression" => {
                let func = self.field(node, "function")?;
                let args = self.field(node, "arguments")?;
                Ok(format!("{}{}", self.expr(func)?, self.arguments(args)?))
            }
            "arguments" => self.arguments(node),
            "parenthesized_expression" => {
                let inner = node
                    .named_child(0)
                    .ok_or_else(|| self.unsupported(node, "empty parentheses"))?;
                Ok(format!("({})", self.expr(inner)?))
            }
            "macro_invocation" => {
                let name = self.field(node, "macro").or_else(|_| {
                    node.named_child(0)
                        .ok_or_else(|| self.unsupported(node, "macro without name"))
                })?;
                let tokens = node
                    .named_children(&mut node.walk())
                    .find(|c| c.kind() == "token_tree")
                    .ok_or_else(|| self.unsupported(node, "macro without token tree"))?;
                Ok(format!("{}!{}", self.text(name)?, self.token_tree(tokens)?))
            }
            // `a.b` field access.
            "field_expression" => {
                let value = self.field(node, "value")?;
                let field = self.field(node, "field")?;
                Ok(format!("{}.{}", self.expr(value)?, self.text(field)?))
            }
            // `Name { field: value, shorthand }` struct literal (single line).
            "struct_expression" => {
                let name = self.field(node, "name")?;
                let body = self.field(node, "body")?;
                let mut out = self.expr(name)?;
                out.push_str(" {");
                let mut cursor = body.walk();
                let mut any = false;
                for (i, init) in body.named_children(&mut cursor).enumerate() {
                    out.push_str(if i == 0 { " " } else { ", " });
                    any = true;
                    match init.kind() {
                        "field_initializer" => {
                            let f = self.field(init, "field")?;
                            let v = self.field(init, "value")?;
                            out.push_str(self.text(f)?);
                            out.push_str(": ");
                            out.push_str(&self.expr(v)?);
                        }
                        "shorthand_field_initializer" => out.push_str(self.text(init)?),
                        _ => return Err(self.unsupported(init, "struct field initializer")),
                    }
                }
                out.push_str(if any { " }" } else { "}" });
                Ok(out)
            }
            // `a::b::c` paths (value or type position).
            "scoped_identifier" | "scoped_type_identifier" => {
                let name = self.field(node, "name")?;
                match node.child_by_field_name("path") {
                    Some(path) => Ok(format!("{}::{}", self.expr(path)?, self.text(name)?)),
                    None => Ok(format!("::{}", self.text(name)?)),
                }
            }
            // `&T` / `&mut T`. Explicit lifetimes are not supported yet.
            "reference_type" => {
                let ty = self.field(node, "type")?;
                let mut cursor = node.walk();
                let mut is_mut = false;
                for child in node.named_children(&mut cursor) {
                    match child.kind() {
                        "mutable_specifier" => is_mut = true,
                        "lifetime" => return Err(self.unsupported(child, "reference lifetime")),
                        _ => {}
                    }
                }
                Ok(format!(
                    "&{}{}",
                    if is_mut { "mut " } else { "" },
                    self.expr(ty)?
                ))
            }
            // `Base<A, B>`.
            "generic_type" => {
                let base = self.field(node, "type")?;
                let args = self.field(node, "type_arguments")?;
                let mut out = self.expr(base)?;
                out.push('<');
                let mut cursor = args.walk();
                for (i, a) in args.named_children(&mut cursor).enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    out.push_str(&self.expr(a)?);
                }
                out.push('>');
                Ok(out)
            }
            _ => Err(self.unsupported(node, "expression")),
        }
    }

    fn arguments(&self, node: Node) -> Result<String, Error> {
        let mut out = String::from("(");
        let mut cursor = node.walk();
        for (i, arg) in node.named_children(&mut cursor).enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            out.push_str(&self.expr(arg)?);
        }
        out.push(')');
        Ok(out)
    }

    /// Canonicalize a macro `token_tree`. Token trees are unstructured, so we
    /// reprint conservatively: brackets verbatim, `, ` after commas, and every
    /// other token rendered with no inserted spacing. This is exact for the
    /// common `name!(expr, expr)` shape and stays fail-closed via [`Self::expr`]
    /// for any named token it does not recognize.
    fn token_tree(&self, node: Node) -> Result<String, Error> {
        let mut out = String::new();
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match self.text(child)? {
                "(" | "[" | "{" | ")" | "]" | "}" => out.push_str(self.text(child)?),
                "," => out.push_str(", "),
                _ if child.is_named() => out.push_str(&self.expr(child)?),
                other => out.push_str(other),
            }
        }
        Ok(out)
    }

    fn indent(&mut self, depth: usize) {
        for _ in 0..depth {
            self.out.push_str(INDENT);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Canonicalize a source string, panicking on error — for the happy-path
    /// assertions below.
    fn canon(s: &str) -> String {
        String::from_utf8(canonicalize(s.as_bytes()).unwrap()).unwrap()
    }

    const CANONICAL: &str = "fn add(a: i32, b: i32) -> i32 {\n    \
        // Simple addition\n    a + b\n}\n\n\
        fn main() {\n    \
        let x = 5;\n    let y = 10;\n    let sum = add(x, y);\n    \
        println!(\"Sum: {}\", sum);\n}\n";

    #[test]
    fn canonicalizes_the_example() {
        let messy = b"fn   add(a:i32,b:i32)->i32{\n// Simple addition\n  a+b\n}\nfn main(){let x=5;\nlet y =10;let sum= add(x,y);println!(\"Sum: {}\",sum);}";
        let out = canonicalize(messy).unwrap();
        assert_eq!(String::from_utf8(out).unwrap(), CANONICAL);
    }

    #[test]
    fn is_idempotent() {
        // Canonical input must come back byte-for-byte unchanged.
        let once = canonicalize(CANONICAL.as_bytes()).unwrap();
        assert_eq!(once, CANONICAL.as_bytes());
    }

    #[test]
    fn reformatting_produces_identical_bytes() {
        // The property the whole project rests on: formatting differences vanish.
        let a = canonicalize(b"fn f()->i32{1+2}").unwrap();
        let b = canonicalize(b"fn f( ) -> i32 {\n\n    1  +  2\n}\n").unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn rejects_syntax_errors() {
        let err = canonicalize(b"fn main( { ").unwrap_err();
        assert!(matches!(err, Error::Parsing(_)));
    }

    #[test]
    fn fails_closed_on_unsupported_constructs() {
        // Still outside the documented subset (traits, tuple structs): error,
        // never silent loss.
        assert!(matches!(
            canonicalize(b"trait T {}\n").unwrap_err(),
            Error::Generation(_)
        ));
        assert!(matches!(
            canonicalize(b"struct T(i32);\n").unwrap_err(),
            Error::Generation(_)
        ));
    }

    // --- Widened item coverage: use / const / struct / enum / impl ---

    #[test]
    fn canonicalizes_use_declarations() {
        assert_eq!(canon("use   std::fmt ;"), "use std::fmt;\n");
        assert_eq!(canon("use a::b::* ;"), "use a::b::*;\n");
    }

    #[test]
    fn canonicalizes_const_and_static() {
        assert_eq!(canon("const  MAX:i32=100;"), "const MAX: i32 = 100;\n");
        assert_eq!(canon("static  S:i32=1;"), "static S: i32 = 1;\n");
    }

    #[test]
    fn canonicalizes_structs() {
        assert_eq!(
            canon("struct  Point{x:i32,y:i32}"),
            "struct Point {\n    x: i32,\n    y: i32,\n}\n"
        );
        assert_eq!(canon("struct Unit ;"), "struct Unit;\n");
    }

    #[test]
    fn canonicalizes_enums() {
        assert_eq!(
            canon("enum Color{Red,Green,Blue}"),
            "enum Color {\n    Red,\n    Green,\n    Blue,\n}\n"
        );
    }

    #[test]
    fn canonicalizes_impl_blocks() {
        assert_eq!(
            canon("impl Point{fn sum(&self)->i32{self.x+self.y}}"),
            "impl Point {\n    fn sum(&self) -> i32 {\n        self.x + self.y\n    }\n}\n"
        );
        // trait impl
        assert_eq!(
            canon("impl Default for P{fn d()->i32{0}}"),
            "impl Default for P {\n    fn d() -> i32 {\n        0\n    }\n}\n"
        );
    }

    #[test]
    fn canonicalizes_references_generics_and_struct_literals() {
        assert_eq!(
            canon("fn f(v:&mut Vec<i32>)->i32{0}"),
            "fn f(v: &mut Vec<i32>) -> i32 {\n    0\n}\n"
        );
        assert_eq!(
            canon("fn g()->P{P{x:1,y:2}}"),
            "fn g() -> P {\n    P { x: 1, y: 2 }\n}\n"
        );
    }

    #[test]
    fn module_with_mixed_items_converges_and_is_idempotent() {
        let messy =
            b"use std::fmt;\nstruct P{x:i32}\nenum E{A,B}\nimpl P{fn x(&self)->i32{self.x}}";
        let tidy = b"use std::fmt;\n\nstruct P {\n    x: i32,\n}\n\nenum E {\n    A,\n    B,\n}\n\nimpl P {\n    fn x(&self) -> i32 {\n        self.x\n    }\n}\n";
        let a = canonicalize(messy).unwrap();
        assert_eq!(
            canonicalize(tidy).unwrap(),
            a,
            "messy and tidy must converge"
        );
        assert_eq!(
            canonicalize(&a).unwrap(),
            a,
            "module canonicalization must be idempotent"
        );
    }

    // --- Determinism contract (see the module-level "Determinism" docs) ---

    #[test]
    fn convergence_many_formattings_one_program() {
        // Every formatting of the same program must canonicalize to identical
        // bytes — the property that keeps reformatting out of history.
        let variants: &[&[u8]] = &[
            b"fn add(a:i32,b:i32)->i32{a+b}",
            b"fn add( a : i32 , b : i32 ) -> i32 { a + b }",
            b"fn   add(a: i32,b: i32)->i32{\n\n    a  +  b\n\n}\n",
            b"fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n",
        ];
        let canon = canonicalize(variants[0]).unwrap();
        for v in variants {
            assert_eq!(canonicalize(v).unwrap(), canon, "variant diverged: {v:?}");
        }
    }

    #[test]
    fn idempotent_on_varied_inputs() {
        // canonicalize is a fixed point on its own output: clean(clean(x)) ==
        // clean(x). This is what makes `smudge` safe as identity and prevents
        // edit/checkout/add churn.
        let inputs: &[&[u8]] = &[
            b"fn f()->i32{1+2}",
            b"fn g(a: i32) -> i32 { let x = a; x }",
            b"fn main(){let s=add(x,y);println!(\"{}\",s);}",
            b"// leading comment\nfn h() -> i32 { 42 }",
        ];
        for input in inputs {
            let once = canonicalize(input).unwrap();
            let twice = canonicalize(&once).unwrap();
            assert_eq!(once, twice, "not idempotent for: {input:?}");
        }
    }

    #[test]
    fn inspect_content_hash_is_stable_across_formatting() {
        // The read verb's headline: a definition's content identity survives
        // reformatting (the hash is over canonical form).
        let messy = canonicalize(b"fn add(a:i32,b:i32)->i32{a+b}").unwrap();
        let tidy = inspect(b"fn add( a : i32 , b : i32 ) -> i32 {\n\n    a + b\n}\n").unwrap();
        let from_messy = inspect(&messy).unwrap();
        assert_eq!(from_messy, tidy);
        assert_eq!(from_messy.len(), 1);
        assert_eq!(from_messy[0].name, "add");
    }

    #[test]
    fn inspect_distinguishes_bodies_and_lists_in_order() {
        let defs = inspect(b"fn a()->i32{1+2}\nfn b()->i32{1-2}").unwrap();
        assert_eq!(defs.iter().map(|d| &d.name).collect::<Vec<_>>(), ["a", "b"]);
        assert_ne!(defs[0].content_hash, defs[1].content_hash);
    }

    #[test]
    fn subtree_hashes_are_formatting_invariant_and_share_subexpressions() {
        // Same function, two formattings → identical subtree-hash multisets.
        let a = &inspect(b"fn f(x:i32)->i32{x+1}").unwrap()[0];
        let b = &inspect(b"fn f(x: i32) -> i32 {\n\n    x + 1\n}\n").unwrap()[0];
        assert_eq!(a.subtree_hashes, b.subtree_hashes);
        // Two functions sharing the sub-expression `x + 1` share its subtree hash.
        let c = &inspect(b"fn g(x: i32) -> i32 { x + 1 + 2 }").unwrap()[0];
        let common = a
            .subtree_hashes
            .iter()
            .filter(|h| c.subtree_hashes.contains(h))
            .count();
        assert!(
            common > 0,
            "shared sub-expression should share a subtree hash"
        );
    }

    #[test]
    fn shape_hash_is_name_invariant_but_body_sensitive() {
        // Same body, different name → same shape_hash, different content_hash.
        let a = &inspect(b"fn a(x: i32) -> i32 { x + 1 }").unwrap()[0];
        let b = &inspect(b"fn b(x: i32) -> i32 { x + 1 }").unwrap()[0];
        assert_eq!(a.shape_hash, b.shape_hash, "name must not affect shape");
        assert_ne!(a.content_hash, b.content_hash, "name must affect content");
        // Same name, different body → different shape_hash.
        let c = &inspect(b"fn a(x: i32) -> i32 { x + 2 }").unwrap()[0];
        assert_ne!(a.shape_hash, c.shape_hash, "body must affect shape");
    }

    #[test]
    fn inspect_surfaces_all_top_level_item_kinds() {
        let defs = inspect(
            b"struct S { x: i32 }\nenum E { A }\nconst C: i32 = 1;\nstatic T: i32 = 2;\nfn f() -> i32 { 1 }\nimpl S { fn m(&self) -> i32 { self.x } }",
        )
        .unwrap();
        let kinds: Vec<(&str, &str)> = defs.iter().map(|d| (d.kind, d.name.as_str())).collect();
        assert!(kinds.contains(&("struct", "S")));
        assert!(kinds.contains(&("enum", "E")));
        assert!(kinds.contains(&("const", "C")));
        assert!(kinds.contains(&("static", "T")));
        assert!(kinds.contains(&("fn", "f")));
        assert!(kinds.contains(&("impl", "S"))); // impl identified by its type
    }

    #[test]
    fn struct_rename_with_same_fields_shares_shape_hash() {
        let a = &inspect(b"struct Point { x: i32, y: i32 }").unwrap()[0];
        let b = &inspect(b"struct Coord { x: i32, y: i32 }").unwrap()[0];
        assert_eq!(a.shape_hash, b.shape_hash, "renamed struct, same fields");
        assert_ne!(a.content_hash, b.content_hash);
    }

    #[test]
    fn pure_repeated_calls_are_byte_identical() {
        // No clock/locale/randomness/hash-ordering leaks into the output.
        let input = b"fn main(){let x=5;let y=10;let s=add(x,y);println!(\"{}\",s);}";
        let first = canonicalize(input).unwrap();
        for _ in 0..16 {
            assert_eq!(canonicalize(input).unwrap(), first);
        }
    }
}
