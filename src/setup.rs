//! One-command installation of the git-ast filter into a repository.
//!
//! `git-ast setup` registers the long-running filter in the current repo's git
//! config and ensures `*.rs` is routed through it in `.gitattributes`, so a user
//! can enable the canonical-formatting round-trip without memorizing the config
//! incantation. It is idempotent: re-running it changes nothing.

use crate::Error;
use std::path::Path;
use std::process::Command;

const ATTR_LINE: &str = "*.rs filter=git-ast";

/// Configure the current repository to use git-ast for `*.rs` files.
pub fn run() -> Result<(), Error> {
    // The filter invokes this same binary; use its absolute path so the config
    // keeps working regardless of the caller's PATH.
    let exe = std::env::current_exe()
        .map_err(|e| Error::Config(format!("cannot locate the git-ast binary: {e}")))?;
    let exe = exe.display();

    git_config("filter.git-ast.process", &format!("{exe} filter-process"))?;
    // `required=true` makes Git fail loudly if the filter is missing rather than
    // silently storing unfiltered bytes.
    git_config("filter.git-ast.required", "true")?;

    ensure_attribute()?;

    eprintln!("git-ast: configured filter for *.rs in this repository.");
    eprintln!("git-ast: re-add existing Rust files to canonicalize them: git add --renormalize .");
    Ok(())
}

fn git_config(key: &str, value: &str) -> Result<(), Error> {
    let status = Command::new("git")
        .args(["config", key, value])
        .status()
        .map_err(|e| Error::Config(format!("running git config: {e}")))?;
    if !status.success() {
        return Err(Error::Config(format!(
            "git config {key} failed (are you inside a git repository?)"
        )));
    }
    Ok(())
}

/// Append the `*.rs filter=git-ast` line to `.gitattributes` unless it is
/// already present.
fn ensure_attribute() -> Result<(), Error> {
    let path = Path::new(".gitattributes");
    let existing = std::fs::read_to_string(path).unwrap_or_default();
    if existing.lines().any(|l| l.trim() == ATTR_LINE) {
        return Ok(());
    }
    let mut updated = existing;
    if !updated.is_empty() && !updated.ends_with('\n') {
        updated.push('\n');
    }
    updated.push_str(ATTR_LINE);
    updated.push('\n');
    std::fs::write(path, updated)?;
    Ok(())
}
