//! Custom diff and merge drivers.
//!
//! The **merge driver** performs a real structural 3-way merge for `*.json`
//! (see [`crate::merge`]): it parses base/ours/theirs, merges by structure, and
//! writes the canonical merged JSON on a clean merge — falling back to standard
//! conflict markers (and a non-zero exit) on a genuine conflict. Other paths keep
//! the conflict-marker placeholder. The **diff driver** is still a placeholder
//! (shells out to `diff -u`).

use crate::merge::Merge3;
use crate::{json, merge, Error};
use std::io::Write;
use std::path::Path;
use std::process::Command;

/// Run the diff driver.
///
/// Git invokes `GIT_EXTERNAL_DIFF`-style with 7 args:
/// `path old-file old-hex old-mode new-file new-hex new-mode`.
///
/// Placeholder: emits a unified text diff of the two files on stdout.
pub fn run_diff_driver(args: &[String]) -> Result<(), Error> {
    if args.len() < 7 {
        return Err(Error::Driver(
            "diff driver expects 7 arguments (path old-file old-hex old-mode new-file new-hex new-mode)".to_string(),
        ));
    }
    let (path, old_file, new_file) = (&args[0], &args[1], &args[4]);
    eprintln!("[diff] {path}: {old_file} -> {new_file} (placeholder: text diff)");

    let output = Command::new("diff")
        .arg("-u")
        .arg(old_file)
        .arg(new_file)
        .output()?;
    std::io::stdout().write_all(&output.stdout)?;

    // `diff` exits 0 when identical and 1 when differing; both are success here.
    match output.status.code() {
        Some(0) | Some(1) => Ok(()),
        _ => Err(Error::Driver(format!("`diff` failed: {:?}", output.status))),
    }
}

/// Run the merge driver.
///
/// Git invokes `git-ast merge-driver %O %A %B %L %P`:
/// base (`%O`), current/ours (`%A`, read+write), other/theirs (`%B`), marker
/// size, pathname (`%P`).
///
/// For `*.json`, attempts a structural 3-way merge ([`crate::merge::merge3`]) and
/// writes the canonical merged JSON to `%A` on success. On a genuine conflict (or
/// for any other path), writes standard conflict markers to `%A` and returns an
/// error so the process exits non-zero — the signal that conflicts remain.
pub fn run_merge_driver(args: &[String]) -> Result<(), Error> {
    if args.len() < 5 {
        return Err(Error::Driver(
            "merge driver expects 5 arguments (%O %A %B %L %P)".to_string(),
        ));
    }
    let base_path = Path::new(&args[0]);
    let current_path = Path::new(&args[1]);
    let other_path = Path::new(&args[2]);
    let pathname = &args[4];

    let base = std::fs::read(base_path)?;
    let current = std::fs::read(current_path)?;
    let other = std::fs::read(other_path)?;

    // Structural merge for JSON; clean result is written canonical.
    if pathname.ends_with(".json") {
        if let Some(merged) = try_json_merge(&base, &current, &other) {
            eprintln!("[merge] {pathname}: structural JSON merge (clean)");
            std::fs::write(current_path, merged)?;
            return Ok(());
        }
        eprintln!("[merge] {pathname}: structural JSON merge — conflict");
    }

    // Conflict (or non-JSON): emit standard conflict markers, exit non-zero.
    write_conflict_markers(current_path, &current, &other)?;
    Err(Error::Driver(format!(
        "{pathname}: merge left unresolved conflicts"
    )))
}

/// Attempt a structural JSON merge. Returns the canonical merged bytes on a clean
/// merge, or `None` on a conflict or if any input is not valid JSON.
fn try_json_merge(base: &[u8], ours: &[u8], theirs: &[u8]) -> Option<Vec<u8>> {
    let base = serde_json::from_slice(base).ok()?;
    let ours = serde_json::from_slice(ours).ok()?;
    let theirs = serde_json::from_slice(theirs).ok()?;
    match merge::merge3(&base, &ours, &theirs) {
        Merge3::Clean(v) => json::canonicalize_value(&v).ok(),
        Merge3::Conflict => None,
    }
}

/// Write standard 3-way conflict markers (ours vs theirs) to `path`.
fn write_conflict_markers(path: &Path, ours: &[u8], theirs: &[u8]) -> Result<(), Error> {
    let mut merged = Vec::new();
    merged.extend_from_slice(b"<<<<<<< HEAD\n");
    merged.extend_from_slice(ours);
    merged.extend_from_slice(b"\n=======\n");
    merged.extend_from_slice(theirs);
    merged.extend_from_slice(b"\n>>>>>>> OTHER\n");
    std::fs::write(path, merged)?;
    Ok(())
}
