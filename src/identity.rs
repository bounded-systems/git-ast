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
//! [`match_defs`] layers these strongest-first, then adds a **structural fuzzy**
//! final pass: leftover defs (a different name *and* a different body) are scored
//! by Sørensen–Dice similarity over their **Merkle subtree-hash multisets**
//! ([`crate::printer::Def::subtree_hashes`]) and paired greedily above a threshold
//! — recognizing a function that was **renamed and edited at once**. Comparing
//! shared *subtrees* (not body text) is GumTree's bottom-up phase: it is
//! formatting- and statement-order-invariant. A full edit *script* (the top-down
//! phase, with move detection) remains the deeper frontier.

use crate::printer::Def;

/// How a definition corresponds across two versions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Correspondence {
    /// Same name and body (content-identical; position ignored).
    Unchanged { name: String },
    /// Same body, different name.
    Renamed { from: String, to: String },
    /// Renamed *and* edited: a different name and a different body, but bodies
    /// similar enough (fuzzy match) to be the same node. `similarity` is a Dice
    /// percentage (0–100) over the name-blanked bodies.
    RenamedEdited {
        from: String,
        to: String,
        similarity: u8,
    },
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
    // Pass 4 (fuzzy): leftover defs paired by body similarity → RenamedEdited.
    // Rank all cross pairs above the threshold by similarity (desc), then assign
    // greedily so each def is matched at most once. Deterministic tie-breaks.
    let leftover = |used: &[bool]| (0..used.len()).filter(|i| !used[*i]).collect::<Vec<_>>();
    let mut ranked: Vec<(f64, usize, usize)> = Vec::new();
    for &oi in &leftover(&old_used) {
        for &ni in &leftover(&new_used) {
            // Structural similarity: shared Merkle subtrees (GumTree bottom-up),
            // not string overlap — formatting- and order-invariant.
            let s = structural_dice(&old[oi].subtree_hashes, &new[ni].subtree_hashes);
            if s >= SIMILARITY_THRESHOLD {
                ranked.push((s, oi, ni));
            }
        }
    }
    ranked.sort_by(|a, b| {
        b.0.partial_cmp(&a.0)
            .unwrap()
            .then(a.1.cmp(&b.1))
            .then(a.2.cmp(&b.2))
    });
    let mut fuzzy: Vec<(usize, usize, f64)> = Vec::new();
    for (s, oi, ni) in ranked {
        if old_used[oi] || new_used[ni] {
            continue;
        }
        old_used[oi] = true;
        new_used[ni] = true;
        fuzzy.push((oi, ni, s));
    }
    fuzzy.sort_by_key(|&(oi, _, _)| oi);
    for (oi, ni, s) in fuzzy {
        out.push(Correspondence::RenamedEdited {
            from: old[oi].name.clone(),
            to: new[ni].name.clone(),
            similarity: (s * 100.0).round() as u8,
        });
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

/// Minimum Dice similarity (0–1) over name-blanked bodies for a fuzzy match.
/// Above this, two differently-named, differently-bodied functions are treated
/// as the same node renamed-and-edited; below it, they are add/remove.
const SIMILARITY_THRESHOLD: f64 = 0.5;

/// Sørensen–Dice coefficient over the two **Merkle subtree-hash multisets**:
/// `2·|A∩B| / (|A|+|B|)`. A structural similarity in `[0, 1]` — shared subtrees
/// (formatting- and order-invariant) count exactly. The inputs are sorted, but we
/// count via a map so the measure is a true multiset intersection.
fn structural_dice(a: &[u64], b: &[u64]) -> f64 {
    if a.is_empty() || b.is_empty() {
        return if a.len() == b.len() { 1.0 } else { 0.0 };
    }
    let mut counts: std::collections::HashMap<u64, i32> = std::collections::HashMap::new();
    for &h in a {
        *counts.entry(h).or_insert(0) += 1;
    }
    let mut inter = 0usize;
    for &h in b {
        let c = counts.entry(h).or_insert(0);
        if *c > 0 {
            *c -= 1;
            inter += 1;
        }
    }
    2.0 * inter as f64 / (a.len() + b.len()) as f64
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
            Correspondence::RenamedEdited {
                from,
                to,
                similarity,
            } => s.push_str(&format!(
                "renamed+   {from} -> {to} ({similarity}% similar)\n"
            )),
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
        // Genuinely dissimilar bodies → below the fuzzy threshold → add/remove.
        let cs = matched(
            "fn gone() -> i32 { 1 }",
            "fn fresh(a: i32, b: i32) -> i32 { a * b - a + b + 42 }",
        );
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
    fn renamed_and_edited_is_a_fuzzy_match() {
        // Different name AND a small body edit → recognized as the same node.
        let cs = matched(
            "fn parseConfig(s: i32) -> i32 { let x = s + 1; x * 2 }",
            "fn loadSettings(s: i32) -> i32 { let x = s + 1; x * 3 }",
        );
        match cs.as_slice() {
            [Correspondence::RenamedEdited {
                from,
                to,
                similarity,
            }] => {
                assert_eq!(
                    (from.as_str(), to.as_str()),
                    ("parseConfig", "loadSettings")
                );
                // Structural Dice: shared subtrees, minus the edited node's
                // ancestor chain. Above the match threshold, below string Dice.
                assert!(*similarity >= 50, "similarity was {similarity}");
            }
            other => panic!("expected one RenamedEdited, got {other:?}"),
        }
    }

    #[test]
    fn reordered_and_edited_matches_structurally() {
        // Statements reordered AND one edited, plus renamed. The Merkle subtree
        // multiset is order-independent, so the moved `let` statements still match
        // — recognized as the same node where string similarity would be shakier.
        let cs = matched(
            "fn f(a: i32, b: i32) -> i32 { let x = a + 1; let y = b + 2; x * y }",
            "fn g(a: i32, b: i32) -> i32 { let y = b + 2; let x = a + 1; x + y }",
        );
        match cs.as_slice() {
            [Correspondence::RenamedEdited { from, to, .. }] => {
                assert_eq!((from.as_str(), to.as_str()), ("f", "g"));
            }
            other => panic!("expected RenamedEdited f -> g, got {other:?}"),
        }
    }

    #[test]
    fn dissimilar_bodies_do_not_fuzzy_match() {
        // Different name and an unrelated body → add/remove, not a fuzzy match.
        let cs = matched(
            "fn alpha() -> i32 { 1 + 2 + 3 }",
            "fn omega(a: i32, b: i32) -> i32 { let p = a + b; let q = a - b; helper(p, q) }",
        );
        assert_eq!(
            cs,
            vec![
                Correspondence::Removed {
                    name: "alpha".into()
                },
                Correspondence::Added {
                    name: "omega".into()
                },
            ]
        );
    }

    #[test]
    fn mixed_change_set() {
        let cs = matched(
            "fn keep()->i32{1}\nfn edit()->i32{2}\nfn old()->i32{3}\nfn drop()->i32{4}",
            "fn keep()->i32{1}\nfn edit()->i32{9}\nfn renamed()->i32{3}\nfn brand(a:i32,b:i32)->i32{a*b+a-b}",
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
    fn recursive_rename_is_recovered_by_fuzzy() {
        // A recursive body references the old name, so the *exact* shape hashes
        // differ (the body changed too). The fuzzy pass recovers it: the bodies
        // are still highly similar, so it reads as renamed-and-edited — not a
        // spurious remove/add.
        let cs = matched(
            "fn fac(n: i32) -> i32 { fac(n) }",
            "fn factorial(n: i32) -> i32 { factorial(n) }",
        );
        match cs.as_slice() {
            [Correspondence::RenamedEdited { from, to, .. }] => {
                assert_eq!((from.as_str(), to.as_str()), ("fac", "factorial"));
            }
            other => panic!("expected RenamedEdited, got {other:?}"),
        }
    }
}
