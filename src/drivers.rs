//! Custom diff and merge drivers.
//!
//! Both are placeholders today: the diff driver shells out to `diff -u`, and the
//! merge driver writes standard conflict markers and reports a conflict. A real
//! implementation would parse each input into a tree and operate structurally.

use crate::Error;
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

    let output = Command::new("diff").arg("-u").arg(old_file).arg(new_file).output()?;
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
/// base, current (read/write), other, marker size, pathname.
///
/// Placeholder: wraps the current and other contents in standard conflict
/// markers, writes them back to `%A`, and returns an error so the process exits
/// non-zero — the correct signal that conflicts remain.
pub fn run_merge_driver(args: &[String]) -> Result<(), Error> {
    if args.len() < 5 {
        return Err(Error::Driver(
            "merge driver expects 5 arguments (%O %A %B %L %P)".to_string(),
        ));
    }
    let current_path = Path::new(&args[1]);
    let other_path = Path::new(&args[2]);
    let pathname = &args[4];
    eprintln!("[merge] {pathname} (placeholder: emit conflict markers)");

    let current = std::fs::read(current_path)?;
    let other = std::fs::read(other_path)?;

    let mut merged = Vec::new();
    merged.extend_from_slice(b"<<<<<<< HEAD\n");
    merged.extend_from_slice(&current);
    merged.extend_from_slice(b"\n=======\n");
    merged.extend_from_slice(&other);
    merged.extend_from_slice(b"\n>>>>>>> OTHER\n");
    std::fs::write(current_path, merged)?;

    Err(Error::Driver("merge left unresolved conflicts (placeholder)".to_string()))
}
