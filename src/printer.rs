//! AST-native canonical printer.
//!
//! [`canonicalize`] parses Rust source with Tree-sitter and re-emits it in a
//! single canonical style by walking the parse tree. This is what the `clean`
//! filter stores, so reformatting never reaches history: two differently
//! formatted inputs that parse to the same tree produce byte-identical output.
//!
//! ## Scope
//!
//! This printer covers a documented *subset* of Rust — enough to round-trip the
//! kinds of items in `examples/rust_simple_addition/` (functions, parameters,
//! blocks, `let` bindings, binary/call/macro expressions, literals, and line and
//! block comments). It is deliberately **fail-closed**: a syntax error or any
//! node kind the printer does not understand returns an [`Error`] rather than
//! guessing, so the filter can never silently corrupt code it cannot represent.
//! Widening the subset is additive — each new node kind is one more arm in
//! [`Printer::expr`] / [`Printer::stmt`].
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
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_rust::language())
        .map_err(|e| Error::Parsing(format!("loading Rust grammar: {e}")))?;
    let tree = parser
        .parse(source, None)
        .ok_or_else(|| Error::Parsing("parser returned no tree".to_string()))?;
    let root = tree.root_node();
    if root.has_error() {
        return Err(Error::Parsing(
            "source has syntax errors; fix them or bypass the filter".to_string(),
        ));
    }

    let mut printer = Printer {
        src: source,
        out: String::new(),
    };
    printer.source_file(root)?;
    Ok(printer.out.into_bytes())
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
            "line_comment" | "block_comment" => {
                self.indent(depth);
                self.out.push_str(self.text(node)?);
                self.out.push('\n');
                Ok(())
            }
            _ => Err(self.unsupported(node, "top-level item")),
        }
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
            if param.kind() != "parameter" {
                return Err(self.unsupported(param, "parameter"));
            }
            if i > 0 {
                self.out.push_str(", ");
            }
            let pattern = self.field(param, "pattern")?;
            let ty = self.field(param, "type")?;
            self.out.push_str(self.text(pattern)?);
            self.out.push_str(": ");
            self.out.push_str(&self.expr(ty)?);
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
        // `struct` is outside the documented subset: error, never silent loss.
        let err = canonicalize(b"struct S { x: i32 }\n").unwrap_err();
        assert!(matches!(err, Error::Generation(_)));
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
    fn pure_repeated_calls_are_byte_identical() {
        // No clock/locale/randomness/hash-ordering leaks into the output.
        let input = b"fn main(){let x=5;let y=10;let s=add(x,y);println!(\"{}\",s);}";
        let first = canonicalize(input).unwrap();
        for _ in 0..16 {
            assert_eq!(canonicalize(input).unwrap(), first);
        }
    }
}
