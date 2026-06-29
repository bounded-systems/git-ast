//! HTML accessibility-tree backend for git-ast.
//!
//! `canonicalize` normalises HTML source — lowercase tags, alphabetically sorted
//! attributes, double-quoted values — so that presentational reformatting never
//! reaches git history. `inspect` extracts the semantic accessibility tree: the
//! ARIA landmarks, headings, and interactive controls that browsers and assistive
//! technologies use, returning one [`Def`] per significant element so that
//! `git-ast match` and `git-ast blame` can track DOM identity across refactors,
//! not line numbers.
//!
//! The ARIA role mapping mirrors lone's `src/adapters/dom.ts`: the implicit role
//! from the tag name, overridden by an explicit `role` attribute. The accessible
//! name follows the same priority: `aria-label` → `title` → `alt` → trimmed text
//! content. This makes git-ast's HTML identity semantically compatible with
//! lone's accessibility validator.

use std::collections::BTreeMap;
use tree_sitter::{Node, Parser};

use crate::printer::Def;
use crate::Error;

// ─── Parsing ─────────────────────────────────────────────────────────────────

fn parse(source: &[u8]) -> Result<tree_sitter::Tree, Error> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_html::LANGUAGE.into())
        .map_err(|e| Error::Parsing(format!("loading HTML grammar: {e}")))?;
    parser
        .parse(source, None)
        .ok_or_else(|| Error::Parsing("HTML parser returned no tree".to_string()))
}

// ─── ARIA semantics ──────────────────────────────────────────────────────────

/// Implicit ARIA role from a lowercase HTML tag name.
/// Mirrors lone `TAG_ROLE_MAP` (`src/adapters/dom.ts`).
fn implicit_role(tag: &str) -> Option<&'static str> {
    match tag {
        "a" => Some("link"),
        "button" => Some("button"),
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => Some("heading"),
        "ul" | "ol" => Some("list"),
        "li" => Some("listitem"),
        "nav" => Some("navigation"),
        "main" => Some("main"),
        "header" => Some("banner"),
        "footer" => Some("contentinfo"),
        "aside" => Some("complementary"),
        "section" | "article" => Some("region"),
        "form" => Some("form"),
        "table" => Some("table"),
        "tr" => Some("row"),
        "th" => Some("columnheader"),
        "td" => Some("cell"),
        "img" => Some("img"),
        "input" | "textarea" | "select" => Some("textbox"),
        "dialog" => Some("dialog"),
        _ => None,
    }
}

/// Canonicalize an explicit `role` attribute value to a `&'static str` suitable
/// for `Def::kind`. Unknown or custom roles return `None` (element is skipped
/// in `inspect` output).
fn aria_kind(role: &str) -> Option<&'static str> {
    match role {
        "alert" => Some("alert"),
        "alertdialog" => Some("alertdialog"),
        "banner" => Some("banner"),
        "button" => Some("button"),
        "cell" => Some("cell"),
        "checkbox" => Some("checkbox"),
        "columnheader" => Some("columnheader"),
        "complementary" => Some("complementary"),
        "contentinfo" => Some("contentinfo"),
        "definition" => Some("definition"),
        "dialog" => Some("dialog"),
        "document" => Some("document"),
        "feed" => Some("feed"),
        "figure" => Some("figure"),
        "form" => Some("form"),
        "grid" => Some("grid"),
        "gridcell" => Some("gridcell"),
        "group" => Some("group"),
        "heading" => Some("heading"),
        "img" => Some("img"),
        "link" => Some("link"),
        "list" => Some("list"),
        "listbox" => Some("listbox"),
        "listitem" => Some("listitem"),
        "log" => Some("log"),
        "main" => Some("main"),
        "marquee" => Some("marquee"),
        "math" => Some("math"),
        "menu" => Some("menu"),
        "menubar" => Some("menubar"),
        "menuitem" => Some("menuitem"),
        "menuitemcheckbox" => Some("menuitemcheckbox"),
        "menuitemradio" => Some("menuitemradio"),
        "navigation" => Some("navigation"),
        "none" | "presentation" => Some("none"),
        "note" => Some("note"),
        "option" => Some("option"),
        "progressbar" => Some("progressbar"),
        "radio" => Some("radio"),
        "radiogroup" => Some("radiogroup"),
        "region" => Some("region"),
        "row" => Some("row"),
        "rowgroup" => Some("rowgroup"),
        "rowheader" => Some("rowheader"),
        "scrollbar" => Some("scrollbar"),
        "search" => Some("search"),
        "searchbox" => Some("searchbox"),
        "separator" => Some("separator"),
        "slider" => Some("slider"),
        "spinbutton" => Some("spinbutton"),
        "status" => Some("status"),
        "switch" => Some("switch"),
        "tab" => Some("tab"),
        "table" => Some("table"),
        "tablist" => Some("tablist"),
        "tabpanel" => Some("tabpanel"),
        "term" => Some("term"),
        "textbox" => Some("textbox"),
        "timer" => Some("timer"),
        "toolbar" => Some("toolbar"),
        "tooltip" => Some("tooltip"),
        "tree" => Some("tree"),
        "treegrid" => Some("treegrid"),
        "treeitem" => Some("treeitem"),
        _ => None,
    }
}

// ─── Attribute utilities ──────────────────────────────────────────────────────

/// Collect attributes from a `start_tag` or `self_closing_tag` node.
/// Returns a `BTreeMap` (keys sorted, lowercased). Values are stripped of their
/// surrounding quotes; boolean attributes (no `=`) map to an empty string.
fn collect_attrs(tag_node: Node, src: &[u8]) -> BTreeMap<String, String> {
    let mut attrs = BTreeMap::new();
    let mut cursor = tag_node.walk();
    for child in tag_node.named_children(&mut cursor) {
        if child.kind() != "attribute" {
            continue;
        }
        let mut ac = child.walk();
        let parts: Vec<Node> = child.named_children(&mut ac).collect();
        let Some(name_node) = parts.first() else {
            continue;
        };
        if name_node.kind() != "attribute_name" {
            continue;
        }
        let name = name_node.utf8_text(src).unwrap_or("").to_lowercase();
        if name.is_empty() {
            continue;
        }
        let value = parts
            .get(1)
            .map(|vn| extract_attr_value(*vn, src))
            .unwrap_or_default();
        attrs.insert(name, value);
    }
    attrs
}

fn extract_attr_value(node: Node, src: &[u8]) -> String {
    match node.kind() {
        "quoted_attribute_value" => {
            // The inner `attribute_value` named child holds the unquoted text.
            // Bind to an owned String inside the block so cursor drops before we
            // use `node` again in the fallback path.
            let found: Option<String> = {
                let mut c = node.walk();
                let x = node
                    .named_children(&mut c)
                    .find(|n| n.kind() == "attribute_value")
                    .and_then(|n| n.utf8_text(src).ok())
                    .map(str::to_string);
                x
            };
            found.unwrap_or_else(|| {
                node.utf8_text(src)
                    .unwrap_or("")
                    .trim_matches(|ch| ch == '"' || ch == '\'')
                    .to_string()
            })
        }
        "attribute_value" => node.utf8_text(src).unwrap_or("").to_string(),
        _ => node
            .utf8_text(src)
            .unwrap_or("")
            .trim_matches(|ch| ch == '"' || ch == '\'')
            .to_string(),
    }
}

// ─── Text content + accessible name ──────────────────────────────────────────

fn collect_text(node: Node, src: &[u8]) -> String {
    let mut buf = String::new();
    collect_text_into(node, src, &mut buf);
    buf
}

fn collect_text_into(node: Node, src: &[u8], buf: &mut String) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "text" => {
                if let Ok(t) = child.utf8_text(src) {
                    buf.push_str(t);
                }
            }
            "element" | "self_closing_element" => collect_text_into(child, src, buf),
            _ => {}
        }
    }
}

/// Accessible name following lone's dom.ts precedence:
/// `aria-label` → `title` → `alt` → collapsed text content.
fn accessible_name(attrs: &BTreeMap<String, String>, text: &str) -> Option<String> {
    for key in &["aria-label", "title", "alt"] {
        if let Some(v) = attrs.get(*key) {
            let t = v.trim();
            if !t.is_empty() {
                return Some(t.to_string());
            }
        }
    }
    let t: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if t.is_empty() {
        None
    } else {
        Some(t)
    }
}

// ─── Hashing ─────────────────────────────────────────────────────────────────

// Local copy of printer's private FNV-1a (same algorithm, same constants).
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

fn fnv1a_hex(bytes: &[u8]) -> String {
    format!("{:016x}", fnv1a(bytes))
}

// ─── Tag name helper ─────────────────────────────────────────────────────────

fn tag_name_of(tag_node: Node, src: &[u8]) -> String {
    // Bind to Option<String> so the cursor borrow ends at the `let` semicolon,
    // before cursor is dropped at the end of the function.
    let found: Option<String> = {
        let mut cursor = tag_node.walk();
        let x = tag_node
            .named_children(&mut cursor)
            .find(|n| n.kind() == "tag_name")
            .and_then(|n| n.utf8_text(src).ok())
            .map(str::to_lowercase);
        x
    };
    found.unwrap_or_default()
}

// ─── Canonicalize ────────────────────────────────────────────────────────────

/// Rewrite HTML to canonical form: lowercase tag names, alphabetically sorted
/// attributes, double-quoted values. Structure and text content are preserved
/// unchanged. Idempotent: `canonicalize(canonicalize(x)) == canonicalize(x)`.
pub fn canonicalize(source: &[u8]) -> Result<Vec<u8>, Error> {
    let tree = parse(source)?;
    let root = tree.root_node();
    let mut out = String::new();
    emit_node(root, source, &mut out);
    if !out.ends_with('\n') {
        out.push('\n');
    }
    Ok(out.into_bytes())
}

fn emit_node(node: Node, src: &[u8], out: &mut String) {
    match node.kind() {
        "document" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                emit_node(child, src, out);
            }
        }
        "element" => emit_element(node, src, out),
        "self_closing_element" => emit_self_closing(node, src, out),
        // Preserve script/style verbatim — canonicalizing their internals is
        // out of scope and would change semantics (e.g. template literals).
        "script_element" | "style_element" => {
            if let Ok(text) = node.utf8_text(src) {
                out.push_str(text);
            }
        }
        "doctype" => out.push_str("<!DOCTYPE html>\n"),
        "text" => {
            if let Ok(text) = node.utf8_text(src) {
                out.push_str(text);
            }
        }
        "comment" => {
            if let Ok(text) = node.utf8_text(src) {
                out.push_str(text);
            }
        }
        _ => {
            if let Ok(text) = node.utf8_text(src) {
                out.push_str(text);
            }
        }
    }
}

fn emit_element(node: Node, src: &[u8], out: &mut String) {
    let mut cursor = node.walk();
    let children: Vec<Node> = node.children(&mut cursor).collect();

    if let Some(&st) = children.iter().find(|n| n.kind() == "start_tag") {
        emit_open_tag(st, src, out, false);
    }
    for &child in children
        .iter()
        .filter(|n| n.kind() != "start_tag" && n.kind() != "end_tag")
    {
        emit_node(child, src, out);
    }
    if let Some(&et) = children.iter().find(|n| n.kind() == "end_tag") {
        let name = tag_name_of(et, src);
        if !name.is_empty() {
            out.push_str(&format!("</{name}>"));
        }
    }
}

fn emit_self_closing(node: Node, src: &[u8], out: &mut String) {
    // Collect children to a Vec so the cursor borrow ends before we call emit_open_tag.
    let mut cursor = node.walk();
    let children: Vec<Node> = node.children(&mut cursor).collect();
    drop(cursor);
    if let Some(&tag) = children.iter().find(|n| n.kind() == "self_closing_tag") {
        emit_open_tag(tag, src, out, true);
    }
}

fn emit_open_tag(tag_node: Node, src: &[u8], out: &mut String, self_close: bool) {
    let tag_name = tag_name_of(tag_node, src);
    let attrs = collect_attrs(tag_node, src);
    out.push('<');
    out.push_str(&tag_name);
    for (k, v) in &attrs {
        if v.is_empty() {
            out.push(' ');
            out.push_str(k);
        } else {
            out.push_str(&format!(r#" {k}="{v}""#));
        }
    }
    if self_close {
        out.push_str(" />");
    } else {
        out.push('>');
    }
}

// ─── Inspect ─────────────────────────────────────────────────────────────────

/// Return the semantic elements of an HTML document as [`Def`]s — one per
/// ARIA landmark, heading, or interactive control — so that `git-ast match`
/// and `git-ast blame` can track accessibility identity across commits.
///
/// The identity axes mirror the Rust backend: `content_hash` couples role +
/// name + attributes (equivalent to Rust's `fn name body`); `shape_hash`
/// blanks the accessible name (equivalent to blanking the declared name), so
/// renamed landmarks (`aria-label` changed) are still recognised as the same
/// structural node. `subtree_hashes` holds one FNV-1a hash per semantic
/// attribute, enabling Dice similarity for the fuzzy matcher.
pub fn inspect(source: &[u8]) -> Result<Vec<Def>, Error> {
    let tree = parse(source)?;
    let root = tree.root_node();
    let mut defs = Vec::new();
    collect_defs(root, source, &mut defs);
    Ok(defs)
}

fn collect_defs(node: Node, src: &[u8], defs: &mut Vec<Def>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "element" => {
                if let Some(def) = element_to_def(child, src) {
                    defs.push(def);
                }
                // Always recurse — nested semantic elements get their own Defs.
                collect_defs(child, src, defs);
            }
            "self_closing_element" => {
                if let Some(def) = self_closing_to_def(child, src) {
                    defs.push(def);
                }
            }
            // Don't recurse into tag nodes — they contain attributes, not elements.
            "start_tag" | "end_tag" | "self_closing_tag" => {}
            _ => collect_defs(child, src, defs),
        }
    }
}

fn element_to_def(node: Node, src: &[u8]) -> Option<Def> {
    // tree-sitter-html 0.20 wraps void elements (<img />) as `element →
    // self_closing_tag` rather than using a separate `self_closing_element`
    // node. Handle both tag kinds here so void elements are not missed.
    let mut cursor = node.walk();
    let children: Vec<Node> = node.children(&mut cursor).collect();
    drop(cursor);
    let (tag_node, has_body) = if let Some(&st) = children.iter().find(|n| n.kind() == "start_tag")
    {
        (st, true)
    } else if let Some(&sc) = children.iter().find(|n| n.kind() == "self_closing_tag") {
        (sc, false)
    } else {
        return None;
    };
    let tag_name = tag_name_of(tag_node, src);
    if tag_name.is_empty() {
        return None;
    }
    let attrs = collect_attrs(tag_node, src);
    let kind = attrs
        .get("role")
        .and_then(|r| aria_kind(r))
        .or_else(|| implicit_role(&tag_name))?;
    let text = if has_body {
        collect_text(node, src)
    } else {
        String::new()
    };
    let name = accessible_name(&attrs, &text).unwrap_or_default();
    Some(make_def(kind, &name, &tag_name, &attrs))
}

fn self_closing_to_def(node: Node, src: &[u8]) -> Option<Def> {
    // In tree-sitter-html ≥0.20, self_closing_element may contain a
    // self_closing_tag child, or it may expose tag_name + attribute directly
    // (grammar version dependent). Fall back to the element itself as tag_node.
    let mut cursor = node.walk();
    let children: Vec<Node> = node.children(&mut cursor).collect();
    let tag_node = children
        .iter()
        .find(|n| n.kind() == "self_closing_tag")
        .copied()
        .unwrap_or(node);
    let tag_name = tag_name_of(tag_node, src);
    if tag_name.is_empty() {
        return None;
    }
    let attrs = collect_attrs(tag_node, src);
    let kind = attrs
        .get("role")
        .and_then(|r| aria_kind(r))
        .or_else(|| implicit_role(&tag_name))?;
    let name = accessible_name(&attrs, "").unwrap_or_default();
    Some(make_def(kind, &name, &tag_name, &attrs))
}

/// Build a [`Def`] from the resolved semantic fields of one HTML element.
///
/// `content_hash`: FNV-1a of `role\0name\0tag\0k=v\0...` (null-separated, attrs
/// sorted). Stable across reformatting; changes when role, name, or any
/// attribute changes.
///
/// `shape_hash`: same preimage with name replaced by `_` — detects a relabelled
/// element (e.g. `aria-label` changed) while the role and structure are the same.
///
/// `subtree_hashes`: one hash per semantic unit (each `k=v` pair, the tag, the
/// role, the name). The Dice similarity in [`crate::identity`] uses these to
/// match elements that were both renamed *and* had their role or structure changed.
/// Attributes whose value contributes to the accessible name; blanked in shape_hash
/// so two elements that differ only in label share the same shape.
const NAME_ATTRS: &[&str] = &["aria-label", "title", "alt"];

fn make_def(kind: &'static str, name: &str, tag: &str, attrs: &BTreeMap<String, String>) -> Def {
    let attr_str: String = attrs
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("\0");

    let shaped_attr_str: String = attrs
        .iter()
        .map(|(k, v)| {
            if NAME_ATTRS.contains(&k.as_str()) {
                format!("{k}=_")
            } else {
                format!("{k}={v}")
            }
        })
        .collect::<Vec<_>>()
        .join("\0");

    let canonical = format!("{kind}\0{name}\0{tag}\0{attr_str}");
    let shaped = format!("{kind}\0_\0{tag}\0{shaped_attr_str}");

    let mut subtree_hashes: Vec<u64> = attrs
        .iter()
        .map(|(k, v)| fnv1a(format!("{k}={v}").as_bytes()))
        .collect();
    subtree_hashes.push(fnv1a(format!("tag={tag}").as_bytes()));
    subtree_hashes.push(fnv1a(format!("role={kind}").as_bytes()));
    if !name.is_empty() {
        subtree_hashes.push(fnv1a(format!("name={name}").as_bytes()));
    }
    subtree_hashes.sort_unstable();

    Def {
        kind,
        name: name.to_string(),
        content_hash: fnv1a_hex(canonical.as_bytes()),
        shape_hash: fnv1a_hex(shaped.as_bytes()),
        subtree_hashes,
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {

    use super::*;

    fn canon(html: &str) -> String {
        String::from_utf8(canonicalize(html.as_bytes()).unwrap()).unwrap()
    }

    fn defs(html: &str) -> Vec<Def> {
        inspect(html.as_bytes()).unwrap()
    }

    // ── canonicalize ──────────────────────────────────────────────────────────

    #[test]
    fn lowercase_tag_names() {
        assert!(canon("<DIV><P>hi</P></DIV>").contains("<div>"));
        assert!(canon("<DIV><P>hi</P></DIV>").contains("<p>"));
    }

    #[test]
    fn attributes_sorted_alphabetically() {
        let out = canon(r#"<button type="button" id="x" aria-pressed="false">ok</button>"#);
        // aria-pressed < id < type
        let ap = out.find("aria-pressed").unwrap();
        let id = out.find("id=").unwrap();
        let ty = out.find("type=").unwrap();
        assert!(ap < id && id < ty, "attrs not sorted: {out}");
    }

    #[test]
    fn values_double_quoted() {
        let out = canon(r#"<a href='/page'>link</a>"#);
        assert!(
            out.contains(r#"href="/page""#),
            "single quotes not converted: {out}"
        );
    }

    #[test]
    fn boolean_attributes_have_no_value() {
        let out = canon(r#"<input disabled type="checkbox" />"#);
        assert!(out.contains(" disabled"), "boolean attr lost: {out}");
        assert!(!out.contains("disabled="), "boolean attr got value: {out}");
    }

    #[test]
    fn canonicalize_is_idempotent() {
        let src = r#"<NAV id="main" aria-label="Main"><a HREF="/home">Home</a></NAV>"#;
        let once = canon(src);
        let twice = canon(&once);
        assert_eq!(once, twice, "not idempotent");
    }

    #[test]
    fn doctype_normalized() {
        let out = canon("<!DOCTYPE HTML><html><body></body></html>");
        assert!(out.starts_with("<!DOCTYPE html>"));
    }

    #[test]
    fn script_and_style_preserved_verbatim() {
        let src = "<script>var x={b:1,a:2};</script>";
        let out = canon(src);
        assert!(
            out.contains("var x={b:1,a:2};"),
            "script was modified: {out}"
        );
    }

    // ── inspect ───────────────────────────────────────────────────────────────

    #[test]
    fn finds_landmark_roles() {
        let ds = defs("<html><body><nav><main></main></nav></body></html>");
        let kinds: Vec<&str> = ds.iter().map(|d| d.kind).collect();
        assert!(kinds.contains(&"navigation"), "nav not found: {kinds:?}");
        assert!(kinds.contains(&"main"), "main not found: {kinds:?}");
    }

    #[test]
    fn finds_headings_and_interactive() {
        let ds = defs("<h1>Title</h1><button>Click</button><a href='/'>Go</a>");
        let kinds: Vec<&str> = ds.iter().map(|d| d.kind).collect();
        assert!(kinds.contains(&"heading"));
        assert!(kinds.contains(&"button"));
        assert!(kinds.contains(&"link"));
    }

    #[test]
    fn accessible_name_from_aria_label() {
        let ds = defs(r#"<nav aria-label="Main navigation"></nav>"#);
        assert_eq!(ds[0].name, "Main navigation");
    }

    #[test]
    fn accessible_name_from_text_content() {
        let ds = defs("<button>  Submit  </button>");
        assert_eq!(ds[0].name, "Submit");
    }

    #[test]
    fn accessible_name_from_alt() {
        let ds = defs(r#"<img alt="Company logo" />"#);
        assert_eq!(ds[0].name, "Company logo");
    }

    #[test]
    fn aria_label_beats_text_content() {
        let ds = defs(r#"<button aria-label="Close dialog">X</button>"#);
        assert_eq!(ds[0].name, "Close dialog");
    }

    #[test]
    fn explicit_role_overrides_tag() {
        let ds = defs(r#"<div role="button">Click me</div>"#);
        assert_eq!(ds[0].kind, "button");
    }

    #[test]
    fn non_semantic_elements_skipped() {
        let ds = defs("<div><span>text</span></div>");
        assert!(ds.is_empty(), "div/span should not produce defs: {ds:?}");
    }

    #[test]
    fn nested_elements_each_get_a_def() {
        let ds = defs("<nav><ul><li><a href='/'>Home</a></li></ul></nav>");
        let kinds: Vec<&str> = ds.iter().map(|d| d.kind).collect();
        assert!(kinds.contains(&"navigation"));
        assert!(kinds.contains(&"list"));
        assert!(kinds.contains(&"listitem"));
        assert!(kinds.contains(&"link"));
    }

    #[test]
    fn content_hash_stable_across_formatting() {
        let compact = defs(r#"<button aria-pressed="false">OK</button>"#);
        let spaced = defs(r#"<button  aria-pressed="false" >OK</button>"#);
        assert_eq!(
            compact[0].content_hash, spaced[0].content_hash,
            "hash changed with whitespace"
        );
    }

    #[test]
    fn shape_hash_differs_from_content_hash_when_named() {
        let ds = defs(r#"<nav aria-label="Main"></nav>"#);
        assert_ne!(
            ds[0].content_hash, ds[0].shape_hash,
            "content_hash and shape_hash should differ when there is a name"
        );
    }

    #[test]
    fn shape_hash_equal_for_same_structure_different_label() {
        let a = defs(r#"<nav aria-label="Main navigation"></nav>"#);
        let b = defs(r#"<nav aria-label="Site navigation"></nav>"#);
        assert_eq!(
            a[0].shape_hash, b[0].shape_hash,
            "shape_hash should match for same structure, different label"
        );
        assert_ne!(
            a[0].content_hash, b[0].content_hash,
            "content_hash must differ for different labels"
        );
    }

    #[test]
    fn subtree_hashes_nonempty_for_semantic_elements() {
        let ds = defs(r#"<nav aria-label="Main"><a href="/">Home</a></nav>"#);
        for d in &ds {
            assert!(
                !d.subtree_hashes.is_empty(),
                "no subtree hashes for {}",
                d.kind
            );
        }
    }
}
