//! One-command installation of the git-ast filter into a repository.
//!
//! `git-ast setup` registers the git-ast filter and merge driver in the current
//! repo's git config and ensures the supported languages are routed through them
//! in `.gitattributes`: the canonical-formatting clean/smudge filter for `*.rs`
//! and `*.json`, plus the **structural merge driver** for `*.json`. A user can
//! enable git-ast without memorizing the config incantation. It is idempotent:
//! re-running it changes nothing.

use crate::Error;
use std::path::Path;
use std::process::Command;

/// `.gitattributes` lines. The clean/smudge filter applies to both languages; the
/// structural merge driver is wired for JSON only (the Rust structural merge is a
/// later increment — routing `*.rs` here would be worse than git's text merge).
const ATTR_LINES: &[&str] = &[
    "*.rs filter=git-ast",
    "*.json filter=git-ast",
    "*.json merge=git-ast",
    "*.json diff=git-ast",
    "*.html filter=git-ast",
    "*.htm filter=git-ast",
];

/// Configure the current repository to use git-ast for the supported languages.
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

    // Structural merge driver (JSON).
    git_config("merge.git-ast.name", "git-ast structural merge")?;
    git_config(
        "merge.git-ast.driver",
        &format!("{exe} merge-driver %O %A %B %L %P"),
    )?;

    // Structural diff driver (JSON). Git appends the GIT_EXTERNAL_DIFF args.
    git_config("diff.git-ast.command", &format!("{exe} diff-driver"))?;

    ensure_attributes()?;

    eprintln!("git-ast: configured filter (*.rs, *.json) + structural merge & diff (*.json).");
    eprintln!("git-ast: re-add existing files to canonicalize them: git add --renormalize .");
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

/// Append each [`ATTR_LINES`] entry to `.gitattributes` unless already present.
fn ensure_attributes() -> Result<(), Error> {
    let path = Path::new(".gitattributes");
    let mut updated = std::fs::read_to_string(path).unwrap_or_default();
    let mut changed = false;
    for line in ATTR_LINES {
        if updated.lines().any(|l| l.trim() == *line) {
            continue;
        }
        if !updated.is_empty() && !updated.ends_with('\n') {
            updated.push('\n');
        }
        updated.push_str(line);
        updated.push('\n');
        changed = true;
    }
    if changed {
        std::fs::write(path, updated)?;
    }
    Ok(())
}
