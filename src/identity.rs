//! Node identity: matching top-level definitions across two versions.
//!
//! The first slice of the hardest open problem (see the node-identity essay in
//! the README). The **lever** is content-addressed hashing: a definition's
//! formatting-invariant hashes ([`crate::printer::Def`]) let us recognize *the
//! same node* across an edit — for free, with no heuristics:
//!
//! - **content_hash** (name + body) equal → the node is **unchanged**. Position
//!   is ignored, so reordering top-level definitions is *not* a change.
//! - **name** equal but content differs → the node is the **same** (a body edit).
//! - **shape_hash** (body, declared name blanked) equal → a **rename** (same body,
//!   new name).
//!
//! [`match_defs`] layers these strongest-first. What it deliberately does *not*
//! do is **fuzzy** matching — a node renamed *and* edited at once — which needs
//! GumTree-family matching and is the genuinely hard remainder of node identity.

use crate::printer::Def;

/// How a definition corresponds across two versions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Correspondence {
    /// Same name and body (content-identical; position ignored).
    Unchanged { name: String },
    /// Same body, different name.
    Renamed { from: String, to: String },
    /// Same name, different body.
    Modified { name: String },
    /// Present only in the new version.
    Added { name: String },
    /// Present only in the old version.
    Removed { name: String },
}

/// Match the definitions of two versions, strongest signal first. Each match
/// consumes one def from each side; remaining defs are added/removed.
pub fn match_defs(old: &[Def], new: &[Def]) -> Vec<Correspondence> {
    let mut old_used = vec![false; old.len()];
    let mut new_used = vec![false; new.len()];
    let mut out = Vec::new();

    // Find the first unused `new` def satisfying `pred`.
    let first_new = |new_used: &[bool], pred: &dyn Fn(&Def) -> bool| -> Option<usize> {
        new.iter()
            .enumerate()
            .find(|(nj, n)| !new_used[*nj] && pred(n))
            .map(|(nj, _)| nj)
    };

    // Pass 1: content_hash equal → Unchanged (exact; position-independent).
    for (oi, o) in old.iter().enumerate() {
        if let Some(ni) = first_new(&new_used, &|n| n.content_hash == o.content_hash) {
            old_used[oi] = true;
            new_used[ni] = true;
            out.push(Correspondence::Unchanged {
                name: new[ni].name.clone(),
            });
        }
    }
    // Pass 2: same name, different body → Modified.
    for (oi, o) in old.iter().enumerate() {
        if old_used[oi] {
            continue;
        }
        if let Some(ni) = first_new(&new_used, &|n| n.name == o.name) {
            old_used[oi] = true;
            new_used[ni] = true;
            out.push(Correspondence::Modified {
                name: o.name.clone(),
            });
        }
    }
    // Pass 3: same body, different name → Renamed (exact).
    for (oi, o) in old.iter().enumerate() {
        if old_used[oi] {
            continue;
        }
        if let Some(ni) = first_new(&new_used, &|n| n.shape_hash == o.shape_hash) {
            old_used[oi] = true;
            new_used[ni] = true;
            out.push(Correspondence::Renamed {
                from: o.name.clone(),
                to: new[ni].name.clone(),
            });
        }
    }
    // Leftovers.
    for (oi, o) in old.iter().enumerate() {
        if !old_used[oi] {
            out.push(Correspondence::Removed {
                name: o.name.clone(),
            });
        }
    }
    for (ni, n) in new.iter().enumerate() {
        if !new_used[ni] {
            out.push(Correspondence::Added {
                name: n.name.clone(),
            });
        }
    }
    out
}

/// Render correspondences as deterministic, human-readable lines.
pub fn render(cs: &[Correspondence]) -> String {
    let mut s = String::new();
    for c in cs {
        match c {
            Correspondence::Unchanged { name } => s.push_str(&format!("unchanged  {name}\n")),
            Correspondence::Renamed { from, to } => {
                s.push_str(&format!("renamed    {from} -> {to}\n"))
            }
            Correspondence::Modified { name } => s.push_str(&format!("modified   {name}\n")),
            Correspondence::Added { name } => s.push_str(&format!("added      {name}\n")),
            Correspondence::Removed { name } => s.push_str(&format!("removed    {name}\n")),
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::printer::inspect;

    fn defs(src: &str) -> Vec<Def> {
        inspect(src.as_bytes()).unwrap()
    }

    fn matched(old: &str, new: &str) -> Vec<Correspondence> {
        match_defs(&defs(old), &defs(new))
    }

    #[test]
    fn unchanged_even_when_reordered() {
        // Same two fns, reversed order, reformatted → both Unchanged (no Moved).
        let cs = matched(
            "fn a()->i32{1}\nfn b()->i32{2}",
            "fn b() -> i32 {\n    2\n}\nfn a()->i32{1}",
        );
        assert_eq!(
            cs,
            vec![
                Correspondence::Unchanged { name: "a".into() },
                Correspondence::Unchanged { name: "b".into() },
            ]
        );
    }

    #[test]
    fn rename_with_unchanged_body() {
        let cs = matched(
            "fn oldName(x: i32) -> i32 { x + 1 }",
            "fn newName(x: i32) -> i32 { x + 1 }",
        );
        assert_eq!(
            cs,
            vec![Correspondence::Renamed {
                from: "oldName".into(),
                to: "newName".into()
            }]
        );
    }

    #[test]
    fn same_name_edited_body_is_modified() {
        let cs = matched("fn f() -> i32 { 1 }", "fn f() -> i32 { 2 }");
        assert_eq!(cs, vec![Correspondence::Modified { name: "f".into() }]);
    }

    #[test]
    fn added_and_removed() {
        let cs = matched("fn gone() -> i32 { 1 }", "fn fresh() -> i32 { 9 }");
        assert_eq!(
            cs,
            vec![
                Correspondence::Removed {
                    name: "gone".into()
                },
                Correspondence::Added {
                    name: "fresh".into()
                },
            ]
        );
    }

    #[test]
    fn mixed_change_set() {
        let cs = matched(
            "fn keep()->i32{1}\nfn edit()->i32{2}\nfn old()->i32{3}\nfn drop()->i32{4}",
            "fn keep()->i32{1}\nfn edit()->i32{9}\nfn renamed()->i32{3}\nfn brand()->i32{5}",
        );
        assert_eq!(
            cs,
            vec![
                Correspondence::Unchanged {
                    name: "keep".into()
                },
                Correspondence::Modified {
                    name: "edit".into()
                },
                Correspondence::Renamed {
                    from: "old".into(),
                    to: "renamed".into()
                },
                Correspondence::Removed {
                    name: "drop".into()
                },
                Correspondence::Added {
                    name: "brand".into()
                },
            ]
        );
    }

    #[test]
    fn name_match_wins_over_rename_when_ambiguous() {
        // old foo(1); new foo(2) + bar(1). foo stays foo (Modified), bar is Added
        // — name-stability beats the body-shape rename signal.
        let cs = matched("fn foo()->i32{1}", "fn foo()->i32{2}\nfn bar()->i32{1}");
        assert_eq!(
            cs,
            vec![
                Correspondence::Modified { name: "foo".into() },
                Correspondence::Added { name: "bar".into() },
            ]
        );
    }

    #[test]
    fn recursive_rename_reads_as_modified_or_removed() {
        // Documented limitation: a recursive body references the old name, so
        // blanking only the declaration does not make the shapes match. The fn
        // is not recognized as a rename.
        let cs = matched(
            "fn fac(n: i32) -> i32 { fac(n) }",
            "fn factorial(n: i32) -> i32 { factorial(n) }",
        );
        assert_eq!(
            cs,
            vec![
                Correspondence::Removed { name: "fac".into() },
                Correspondence::Added {
                    name: "factorial".into()
                },
            ]
        );
    }
}
