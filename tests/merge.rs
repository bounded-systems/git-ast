//! Structural JSON merge: conformance vectors + property tests.
//!
//! The vectors in `tests/merge_vectors.json` are written to be the **shared spec**
//! between this Rust implementation and a forthcoming Lean proof: Rust *executes*
//! each case here; the Lean proof (fast-follow) will *decide* the same cases. The
//! property tests below state the soundness properties (idempotence, only-one-
//! side, symmetry) that Lean will prove universally.

use git_ast::merge::{merge3, Merge3};
use serde_json::Value;
use std::fs;
use std::path::Path;

fn vectors() -> Vec<Value> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/merge_vectors.json");
    let raw = fs::read(path).expect("read merge_vectors.json");
    match serde_json::from_slice(&raw).expect("parse vectors") {
        Value::Array(a) => a,
        _ => panic!("merge_vectors.json must be an array"),
    }
}

#[test]
fn conformance_vectors_match_expectation() {
    for v in vectors() {
        let name = v["name"].as_str().unwrap_or("?");
        let got = merge3(&v["base"], &v["ours"], &v["theirs"]);
        let expect = &v["expect"];
        if expect.get("conflict").and_then(Value::as_bool) == Some(true) {
            assert_eq!(got, Merge3::Conflict, "vector `{name}`: expected conflict");
        } else {
            let want = expect["clean"].clone();
            assert_eq!(
                got,
                Merge3::Clean(want),
                "vector `{name}`: expected clean merge"
            );
        }
    }
}

/// A small, varied sample set for the property tests (no proptest dependency —
/// these mirror universally-proven Lean theorems, so a representative sample is
/// enough to catch a regression in the Rust impl).
fn samples() -> Vec<Value> {
    use serde_json::json;
    vec![
        json!(null),
        json!(true),
        json!(1),
        json!("s"),
        json!([1, 2, 3]),
        json!({"a": 1}),
        json!({"a": 1, "b": {"c": 2}}),
        json!({"x": [1, {"y": 2}], "z": "k"}),
    ]
}

#[test]
fn idempotence_merge3_v_v_v_is_v() {
    for v in samples() {
        assert_eq!(
            merge3(&v, &v, &v),
            Merge3::Clean(v.clone()),
            "merge3(v,v,v) == v"
        );
    }
}

#[test]
fn only_ours_changed_takes_ours() {
    let base = serde_json::json!({"k": 0});
    for v in samples() {
        // theirs == base, ours == v  ->  result is ours (v).
        assert_eq!(merge3(&base, &v, &base), Merge3::Clean(v.clone()));
    }
}

#[test]
fn only_theirs_changed_takes_theirs() {
    let base = serde_json::json!({"k": 0});
    for v in samples() {
        assert_eq!(merge3(&base, &base, &v), Merge3::Clean(v.clone()));
    }
}

#[test]
fn symmetry_swapping_ours_and_theirs() {
    // merge3(o,a,b) and merge3(o,b,a) agree (same clean value, or both conflict).
    let base = serde_json::json!({"a": 0, "b": 0, "o": {"x": 0, "y": 0}});
    let s = samples();
    for a in &s {
        for b in &s {
            assert_eq!(
                merge3(&base, a, b),
                merge3(&base, b, a),
                "symmetry failed for a={a}, b={b}"
            );
        }
    }
}
