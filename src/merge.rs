//! Structural 3-way merge for JSON.
//!
//! This is the *semantic* half of git-ast: where the clean/smudge filter
//! ([`crate::json`]) removes formatting noise, [`merge3`] merges two JSON values
//! against a common base by **structure**, not text lines. Its win over a text
//! merge of canonical JSON is **key-granular merging regardless of textual
//! adjacency**: edits or additions to *different* object keys never conflict,
//! even when they land on adjacent lines; only a genuine same-key divergence
//! conflicts.
//!
//! The algorithm is the standard recursive 3-way rule, with object keys handled
//! as `Option` (present/absent) so add / delete / edit all fall out of one case:
//!
//! ```text
//! merge3(base, ours, theirs):
//!   ours == theirs              -> Clean(ours)      (same change, or no change)
//!   base == ours                -> Clean(theirs)    (only theirs changed)
//!   base == theirs              -> Clean(ours)      (only ours changed)
//!   all three are objects       -> merge key-by-key (recurse); any key conflict -> Conflict
//!   otherwise                   -> Conflict         (divergent scalars/arrays/types)
//! ```
//!
//! The algorithm's soundness properties (idempotence, only-one-side, symmetry)
//! are exercised as Rust property tests in `tests/merge.rs`. A machine-checked
//! **Lean** proof of the same properties — the formal half of "backed by Rust
//! *and* Lean" — is the immediate follow-up; the conformance vectors in
//! `tests/merge_vectors.json` are written to be the shared spec both will run.
//!
//! Boundary (v1): arrays are compared whole — a both-sides-changed array is a
//! conflict (no element-level LCS yet).

use serde_json::{Map, Value};

/// Outcome of a structural 3-way merge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Merge3 {
    /// A clean merge producing this value.
    Clean(Value),
    /// The sides diverged irreconcilably.
    Conflict,
}

/// 3-way merge of `ours` and `theirs` against their common `base`.
pub fn merge3(base: &Value, ours: &Value, theirs: &Value) -> Merge3 {
    // Same change on both sides (covers "no change" too).
    if ours == theirs {
        return Merge3::Clean(ours.clone());
    }
    // Exactly one side changed: take that side.
    if base == ours {
        return Merge3::Clean(theirs.clone());
    }
    if base == theirs {
        return Merge3::Clean(ours.clone());
    }
    // Both changed differently. Only objects can be reconciled structurally.
    if let (Value::Object(b), Value::Object(o), Value::Object(t)) = (base, ours, theirs) {
        return merge_objects(b, o, t);
    }
    Merge3::Conflict
}

/// Per-key merge of three objects. Absent keys are `None`, so a key added on one
/// side, deleted on one side, or edited is all the same `merge3_opt` rule.
fn merge_objects(
    base: &Map<String, Value>,
    ours: &Map<String, Value>,
    theirs: &Map<String, Value>,
) -> Merge3 {
    // Union of keys across all three, deterministically ordered (BTreeMap-backed
    // Maps already iterate sorted; collect into a sorted set to be explicit).
    let mut keys: Vec<&String> = base
        .keys()
        .chain(ours.keys())
        .chain(theirs.keys())
        .collect();
    keys.sort_unstable();
    keys.dedup();

    let mut merged = Map::new();
    for k in keys {
        match merge3_opt(base.get(k), ours.get(k), theirs.get(k)) {
            Some(Merge3::Clean(v)) => {
                merged.insert(k.clone(), v);
            }
            Some(Merge3::Conflict) => return Merge3::Conflict,
            None => { /* key absent in the merged result (deleted) */ }
        }
    }
    Merge3::Clean(Value::Object(merged))
}

/// 3-way merge of a single (possibly absent) key. `None` means the key is absent
/// in the result; `Some(Clean(v))` means it is present with value `v`.
fn merge3_opt(
    base: Option<&Value>,
    ours: Option<&Value>,
    theirs: Option<&Value>,
) -> Option<Merge3> {
    // Same on both sides (present-equal, or both absent).
    if ours == theirs {
        return ours.map(|v| Merge3::Clean(v.clone()));
    }
    // Only theirs changed (incl. theirs deleting a key ours left at base).
    if base == ours {
        return theirs.map(|v| Merge3::Clean(v.clone()));
    }
    // Only ours changed.
    if base == theirs {
        return ours.map(|v| Merge3::Clean(v.clone()));
    }
    // Both present and both changed: recurse (objects reconcile, else conflict).
    if let (Some(b), Some(o), Some(t)) = (base, ours, theirs) {
        return Some(merge3(b, o, t));
    }
    // Edit/delete or add/add divergence.
    Some(Merge3::Conflict)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn m(b: Value, o: Value, t: Value) -> Merge3 {
        merge3(&b, &o, &t)
    }

    #[test]
    fn no_change_is_clean_base() {
        assert_eq!(
            m(json!({"a": 1}), json!({"a": 1}), json!({"a": 1})),
            Merge3::Clean(json!({"a": 1}))
        );
    }

    #[test]
    fn only_ours_changed() {
        assert_eq!(
            m(json!({"a": 1}), json!({"a": 2}), json!({"a": 1})),
            Merge3::Clean(json!({"a": 2}))
        );
    }

    #[test]
    fn only_theirs_changed() {
        assert_eq!(
            m(json!({"a": 1}), json!({"a": 1}), json!({"a": 9})),
            Merge3::Clean(json!({"a": 9}))
        );
    }

    #[test]
    fn different_keys_merge_cleanly() {
        // The headline property: ours edits a, theirs edits b -> both kept.
        let r = m(
            json!({"a": 1, "b": 1}),
            json!({"a": 2, "b": 1}),
            json!({"a": 1, "b": 3}),
        );
        assert_eq!(r, Merge3::Clean(json!({"a": 2, "b": 3})));
    }

    #[test]
    fn same_key_diverges_conflicts() {
        assert_eq!(
            m(json!({"a": 1}), json!({"a": 2}), json!({"a": 3})),
            Merge3::Conflict
        );
    }

    #[test]
    fn add_distinct_keys_merges() {
        let r = m(json!({}), json!({"a": 1}), json!({"b": 2}));
        assert_eq!(r, Merge3::Clean(json!({"a": 1, "b": 2})));
    }

    #[test]
    fn add_same_key_differently_conflicts() {
        assert_eq!(
            m(json!({}), json!({"a": 1}), json!({"a": 2}),),
            Merge3::Conflict
        );
    }

    #[test]
    fn delete_one_side_keep_other_unchanged_deletes() {
        // ours deletes "a", theirs leaves it at base -> deleted.
        let r = m(
            json!({"a": 1, "b": 1}),
            json!({"b": 1}),
            json!({"a": 1, "b": 1}),
        );
        assert_eq!(r, Merge3::Clean(json!({"b": 1})));
    }

    #[test]
    fn edit_delete_conflicts() {
        // ours edits "a", theirs deletes "a" -> conflict.
        assert_eq!(
            m(json!({"a": 1}), json!({"a": 2}), json!({})),
            Merge3::Conflict
        );
    }

    #[test]
    fn nested_objects_different_subkeys_merge() {
        let r = m(
            json!({"o": {"x": 1, "y": 1}}),
            json!({"o": {"x": 2, "y": 1}}),
            json!({"o": {"x": 1, "y": 3}}),
        );
        assert_eq!(r, Merge3::Clean(json!({"o": {"x": 2, "y": 3}})));
    }

    #[test]
    fn both_changed_arrays_conflict() {
        // v1 boundary: no element LCS, so divergent array edits conflict.
        assert_eq!(
            m(
                json!({"a": [1]}),
                json!({"a": [1, 2]}),
                json!({"a": [1, 3]})
            ),
            Merge3::Conflict
        );
    }

    #[test]
    fn symmetry_ours_theirs() {
        // merge3(o,a,b) == merge3(o,b,a) for both clean and conflict outcomes.
        let cases = [
            (json!({"a": 1}), json!({"a": 2}), json!({"a": 1})),
            (
                json!({"a": 1, "b": 1}),
                json!({"a": 2, "b": 1}),
                json!({"a": 1, "b": 3}),
            ),
            (json!({"a": 1}), json!({"a": 2}), json!({"a": 3})),
        ];
        for (b, o, t) in cases {
            assert_eq!(merge3(&b, &o, &t), merge3(&b, &t, &o));
        }
    }
}
