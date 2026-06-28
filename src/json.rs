//! JSON canonicalizer.
//!
//! The companion to [`crate::printer`] (which canonicalizes Rust): same contract,
//! a different language. [`canonicalize`] parses JSON and re-emits a deterministic
//! canonical form — **object keys sorted, pretty-printed, trailing newline** — so
//! that two differently-formatted-but-equal JSON files store byte-identical blobs.
//!
//! Two facts make this a faithful canonical form:
//!
//! - **Sorted keys.** `serde_json::Map` is a `BTreeMap` (the `preserve_order`
//!   feature is off), so keys are emitted in a stable order regardless of input.
//! - **Deterministic scalars.** `serde_json` formats numbers and strings
//!   deterministically, so a given value always renders to the same bytes.
//!
//! This is RFC 8785 (JCS) *value-level* normalization — sorted keys, canonical
//! scalars — rendered *pretty* rather than compact, because the filter's purpose
//! is cleaner diffs and one value per line diffs far better than a single line.
//!
//! Like the Rust path it is **fail-closed**: unparseable JSON returns an error,
//! so `git add` aborts rather than storing junk. `smudge` is the identity (the
//! stored blob is already canonical source); see [`crate::filters`].

use crate::Error;

/// Parse `source` as JSON and return its canonical form (sorted-key pretty JSON
/// with a trailing newline). Returns [`Error::Parsing`] if `source` is not valid
/// JSON.
pub fn canonicalize(source: &[u8]) -> Result<Vec<u8>, Error> {
    let value: serde_json::Value =
        serde_json::from_slice(source).map_err(|e| Error::Parsing(format!("invalid JSON: {e}")))?;
    let mut out =
        serde_json::to_vec_pretty(&value).map_err(|e| Error::Serialization(e.to_string()))?;
    out.push(b'\n');
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn canon(s: &str) -> String {
        String::from_utf8(canonicalize(s.as_bytes()).unwrap()).unwrap()
    }

    #[test]
    fn sorts_keys_and_normalizes_whitespace() {
        assert_eq!(
            canon(r#"{ "b": 1,    "a": 2 }"#),
            "{\n  \"a\": 2,\n  \"b\": 1\n}\n"
        );
    }

    #[test]
    fn is_idempotent() {
        let once = canonicalize(br#"{"z":[3,2,1],"a":{"d":4,"c":3}}"#).unwrap();
        let twice = canonicalize(&once).unwrap();
        assert_eq!(once, twice);
    }

    #[test]
    fn key_order_in_input_does_not_affect_output() {
        let a = canon(r#"{"k2":2,"k1":1,"k3":{"n":[1,2,3]}}"#);
        let b = canon(r#"{"k3":{"n":[1,2,3]},"k1":1,"k2":2}"#);
        assert_eq!(a, b);
    }

    #[test]
    fn preserves_value_semantics() {
        let src = r#"{ "b": 1, "a": [ {"y": true, "x": null} ], "s": "hi\n" }"#;
        let before: serde_json::Value = serde_json::from_str(src).unwrap();
        let after: serde_json::Value =
            serde_json::from_slice(&canonicalize(src.as_bytes()).unwrap()).unwrap();
        assert_eq!(before, after);
    }

    #[test]
    fn ends_with_single_newline() {
        let out = canonicalize(br#"{"a":1}"#).unwrap();
        assert_eq!(out.last(), Some(&b'\n'));
        assert_ne!(out[out.len() - 2], b'\n');
    }

    #[test]
    fn rejects_invalid_json() {
        assert!(matches!(
            canonicalize(b"{not json,}"),
            Err(Error::Parsing(_))
        ));
    }
}
