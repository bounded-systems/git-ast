//! Structural diff for JSON.
//!
//! The companion to [`crate::merge`]: where the merge *reconciles* two JSON values
//! against a base, [`diff`] *describes* the change between two values by
//! **structure** rather than text lines. It reports object-key paths that were
//! added, removed, or changed — order-independent and explicit — so a `git diff`
//! shows *what* changed semantically, not just which lines moved.
//!
//! Wired as Git's external diff driver for `*.json` (see [`crate::drivers`] and
//! `git-ast setup`). Boundary (v1): arrays and differing scalars are reported as
//! a whole-value change (no element-level diff yet) — the same one-level scope as
//! the merge.

use serde_json::Value;

/// A single structural change at an object-key path (dotted, e.g. `a.b.c`; the
/// empty path is the document root).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    Added {
        path: String,
        value: Value,
    },
    Removed {
        path: String,
        value: Value,
    },
    Changed {
        path: String,
        old: Value,
        new: Value,
    },
}

/// Compute the structural diff from `old` to `new`.
pub fn diff(old: &Value, new: &Value) -> Vec<Change> {
    let mut out = Vec::new();
    diff_into("", old, new, &mut out);
    out
}

fn diff_into(path: &str, old: &Value, new: &Value, out: &mut Vec<Change>) {
    if old == new {
        return;
    }
    match (old, new) {
        (Value::Object(o), Value::Object(n)) => {
            // Union of keys, deterministically ordered (Map is a BTreeMap, so its
            // keys already iterate sorted; merge the two sorted sets).
            let mut keys: Vec<&String> = o.keys().chain(n.keys()).collect();
            keys.sort_unstable();
            keys.dedup();
            for k in keys {
                let child = if path.is_empty() {
                    k.clone()
                } else {
                    format!("{path}.{k}")
                };
                match (o.get(k), n.get(k)) {
                    (Some(ov), Some(nv)) => diff_into(&child, ov, nv, out),
                    (None, Some(nv)) => out.push(Change::Added {
                        path: child,
                        value: nv.clone(),
                    }),
                    (Some(ov), None) => out.push(Change::Removed {
                        path: child,
                        value: ov.clone(),
                    }),
                    (None, None) => {}
                }
            }
        }
        // Differing scalars, arrays, or a type change: a whole-value change.
        _ => out.push(Change::Changed {
            path: path.to_string(),
            old: old.clone(),
            new: new.clone(),
        }),
    }
}

/// Render a structural diff as deterministic, human-readable text:
/// `+ path: value` (added), `- path: value` (removed), `~ path: old -> new`.
pub fn render(changes: &[Change]) -> String {
    let mut s = String::new();
    for c in changes {
        match c {
            Change::Added { path, value } => {
                s.push_str(&format!("+ {}: {}\n", label(path), compact(value)));
            }
            Change::Removed { path, value } => {
                s.push_str(&format!("- {}: {}\n", label(path), compact(value)));
            }
            Change::Changed { path, old, new } => {
                s.push_str(&format!(
                    "~ {}: {} -> {}\n",
                    label(path),
                    compact(old),
                    compact(new)
                ));
            }
        }
    }
    s
}

fn label(path: &str) -> &str {
    if path.is_empty() {
        "(root)"
    } else {
        path
    }
}

/// Compact one-line rendering of a value (for the diff lines).
fn compact(v: &Value) -> String {
    serde_json::to_string(v).unwrap_or_else(|_| "<unrenderable>".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn rendered(old: Value, new: Value) -> String {
        render(&diff(&old, &new))
    }

    #[test]
    fn no_change_is_empty() {
        assert_eq!(diff(&json!({"a": 1}), &json!({"a": 1})), vec![]);
    }

    #[test]
    fn changed_scalar_at_key() {
        assert_eq!(rendered(json!({"a": 1}), json!({"a": 2})), "~ a: 1 -> 2\n");
    }

    #[test]
    fn added_and_removed_keys() {
        // ordered by key; only the actual changes appear.
        assert_eq!(
            rendered(json!({"a": 1, "b": 2}), json!({"a": 1, "c": 3})),
            "- b: 2\n+ c: 3\n"
        );
    }

    #[test]
    fn nested_path_is_dotted() {
        assert_eq!(
            rendered(
                json!({"o": {"x": 1, "y": 1}}),
                json!({"o": {"x": 2, "y": 1}})
            ),
            "~ o.x: 1 -> 2\n"
        );
    }

    #[test]
    fn key_reorder_is_no_diff() {
        // Structure, not text: reordering keys produces no change.
        assert_eq!(
            diff(&json!({"a": 1, "b": 2}), &json!({"b": 2, "a": 1})),
            vec![]
        );
    }

    #[test]
    fn array_change_is_whole_value() {
        // v1 boundary: arrays diff as a whole value.
        assert_eq!(
            rendered(json!({"a": [1, 2]}), json!({"a": [1, 2, 3]})),
            "~ a: [1,2] -> [1,2,3]\n"
        );
    }

    #[test]
    fn root_scalar_change() {
        assert_eq!(rendered(json!(1), json!(2)), "~ (root): 1 -> 2\n");
    }

    #[test]
    fn type_change_at_key() {
        assert_eq!(
            rendered(json!({"a": 1}), json!({"a": "x"})),
            "~ a: 1 -> \"x\"\n"
        );
    }
}
