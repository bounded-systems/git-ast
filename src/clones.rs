//! Structural clone detection via the identity index.
//!
//! The `subtree_hashes` and `shape_hash` produced by [`crate::printer::inspect`]
//! and [`crate::html::inspect`] are computed per-file and then discarded. This
//! module accumulates them across a set of files and surfaces **equivalence
//! classes**: groups of definitions that are structurally identical (same body,
//! possibly different names) — Type-1 clones by construction, zero heuristics.
//!
//! The identity-index research note (`docs/research/identity-index.md`) describes
//! why this falls out for free: equal `shape_hash` ⟹ same trie node in the
//! Merkle DAG. This module is the first concrete step toward that index — an
//! in-memory occurrence map over a provided file set, exposing the equivalence
//! class query that the index would answer in O(1) against a persisted store.

use std::collections::BTreeMap;

use crate::printer::Def;

/// A group of structurally identical definitions found across the scanned files.
///
/// All members share the same `shape_hash`, meaning their bodies are identical
/// under git-ast's canonical form — they differ at most by their declared names.
/// This is an exact equivalence class by construction (hash collision ⟹ same
/// canonical body), not a heuristic similarity score.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloneGroup {
    /// The shared shape hash that identifies this equivalence class.
    pub shape_hash: String,
    /// The ARIA role / Rust kind shared by all members (`fn`, `navigation`, …).
    pub kind: &'static str,
    /// All occurrences, in file-then-definition order.
    pub occurrences: Vec<Occurrence>,
}

/// One occurrence of a cloned definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Occurrence {
    /// The file path the definition was found in.
    pub file: String,
    /// The declared name (accessible name for HTML, identifier for Rust).
    pub name: String,
}

/// Build clone groups from an already-inspected set of files.
///
/// `file_defs` is a slice of `(path, defs)` pairs, typically produced by
/// calling `inspect` on each file. Returns only groups with **two or more**
/// occurrences, sorted deterministically by `shape_hash`.
///
/// # Note on exactness
///
/// The grouping is **exact**: equal `shape_hash` means the canonical forms are
/// identical modulo the declared name. No threshold, no approximation. The
/// fuzzy / near-clone layer (MinHash + LSH over `subtree_hashes`) is a
/// separate concern described in the research note.
pub fn find_clones<'a>(file_defs: &[(&'a str, &'a [Def])]) -> Vec<CloneGroup> {
    // shape_hash → (kind, Vec<Occurrence>)
    // BTreeMap gives deterministic iteration order in the output.
    let mut index: BTreeMap<String, (&'static str, Vec<Occurrence>)> = BTreeMap::new();

    for &(file, defs) in file_defs {
        for def in defs {
            let entry = index
                .entry(def.shape_hash.clone())
                .or_insert_with(|| (def.kind, Vec::new()));
            entry.1.push(Occurrence {
                file: file.to_string(),
                name: def.name.clone(),
            });
        }
    }

    index
        .into_iter()
        .filter(|(_, (_, occs))| occs.len() > 1)
        .map(|(shape_hash, (kind, occurrences))| CloneGroup {
            shape_hash,
            kind,
            occurrences,
        })
        .collect()
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn def(kind: &'static str, name: &str, content_hash: &str, shape_hash: &str) -> Def {
        Def {
            kind,
            name: name.to_string(),
            content_hash: content_hash.to_string(),
            shape_hash: shape_hash.to_string(),
            subtree_hashes: vec![],
        }
    }

    #[test]
    fn no_files_gives_no_groups() {
        assert!(find_clones(&[]).is_empty());
    }

    #[test]
    fn single_file_no_duplicates_gives_no_groups() {
        let defs = vec![
            def("fn", "foo", "aaa", "bbb"),
            def("fn", "bar", "ccc", "ddd"),
        ];
        let groups = find_clones(&[("a.rs", &defs)]);
        assert!(groups.is_empty());
    }

    #[test]
    fn same_shape_in_one_file_is_a_clone() {
        let defs = vec![
            def("fn", "foo", "aaa", "shared"),
            def("fn", "bar", "bbb", "shared"),
        ];
        let groups = find_clones(&[("a.rs", &defs)]);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].shape_hash, "shared");
        assert_eq!(groups[0].occurrences.len(), 2);
        assert_eq!(groups[0].occurrences[0].name, "foo");
        assert_eq!(groups[0].occurrences[1].name, "bar");
    }

    #[test]
    fn same_shape_across_two_files_is_a_clone() {
        let a = vec![def("fn", "foo", "aaa", "shared")];
        let b = vec![def("fn", "bar", "bbb", "shared")];
        let groups = find_clones(&[("a.rs", &a), ("b.rs", &b)]);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].occurrences[0].file, "a.rs");
        assert_eq!(groups[0].occurrences[1].file, "b.rs");
    }

    #[test]
    fn content_hash_exact_clone_also_groups_by_shape() {
        // Two fns with identical name AND body → same content_hash AND shape_hash.
        let a = vec![def("fn", "foo", "same", "same")];
        let b = vec![def("fn", "foo", "same", "same")];
        let groups = find_clones(&[("a.rs", &a), ("b.rs", &b)]);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].occurrences.len(), 2);
    }

    #[test]
    fn three_clones_form_one_group() {
        let a = vec![def("fn", "foo", "a1", "s1")];
        let b = vec![def("fn", "bar", "b1", "s1")];
        let c = vec![def("fn", "baz", "c1", "s1")];
        let groups = find_clones(&[("a.rs", &a), ("b.rs", &b), ("c.rs", &c)]);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].occurrences.len(), 3);
    }

    #[test]
    fn different_shapes_produce_separate_groups() {
        let defs = vec![
            def("fn", "foo", "a", "s1"),
            def("fn", "bar", "b", "s1"),
            def("fn", "baz", "c", "s2"),
            def("fn", "qux", "d", "s2"),
        ];
        let groups = find_clones(&[("x.rs", &defs)]);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].shape_hash, "s1");
        assert_eq!(groups[1].shape_hash, "s2");
    }

    #[test]
    fn unique_definitions_are_not_reported() {
        let a = vec![def("fn", "foo", "unique1", "unique1")];
        let b = vec![def("fn", "bar", "unique2", "unique2")];
        let groups = find_clones(&[("a.rs", &a), ("b.rs", &b)]);
        assert!(groups.is_empty());
    }

    #[test]
    fn kind_is_preserved_from_first_occurrence() {
        let a = vec![def("navigation", "Main nav", "h1", "s1")];
        let b = vec![def("navigation", "Site nav", "h2", "s1")];
        let groups = find_clones(&[("a.html", &a), ("b.html", &b)]);
        assert_eq!(groups[0].kind, "navigation");
    }

    #[test]
    fn output_is_sorted_by_shape_hash() {
        let defs = vec![
            def("fn", "a", "x", "zzz"),
            def("fn", "b", "y", "zzz"),
            def("fn", "c", "p", "aaa"),
            def("fn", "d", "q", "aaa"),
        ];
        let groups = find_clones(&[("f.rs", &defs)]);
        assert_eq!(groups[0].shape_hash, "aaa");
        assert_eq!(groups[1].shape_hash, "zzz");
    }

    #[test]
    fn round_trip_through_real_rust_source() {
        use crate::printer;
        let src_a = b"fn helper() -> u32 { 42 }";
        let src_b = b"fn aux() -> u32 { 42 }";
        let defs_a = printer::inspect(src_a).unwrap();
        let defs_b = printer::inspect(src_b).unwrap();
        // Same body, different names → identical shape_hash.
        assert_eq!(defs_a[0].shape_hash, defs_b[0].shape_hash);
        let groups = find_clones(&[("a.rs", &defs_a), ("b.rs", &defs_b)]);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].kind, "fn");
    }
}
