//! Refactor-aware blame, **computed on demand**.
//!
//! For each top-level item in a file's committed version, [`blame`] reports the
//! commit that last changed it — *following it through renames*, so a pure rename
//! does not reset its history (you see the last real **body** change). This is the
//! payoff of node identity ([`crate::identity`]): per git-ast's design essay,
//! *identity is computed, not stored* — so no `git notes` persistence is needed.
//! We walk the file's history and match items commit-to-commit with [`match_defs`].
//!
//! Granularity is **per-definition** (not per-line). Git is shelled in the current
//! working directory (mirroring [`crate::setup`]).

use crate::identity::{match_defs, Correspondence};
use crate::printer::{inspect, Def};
use crate::Error;
use std::process::Command;

/// The commit that last changed one definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Blame {
    /// Short commit hash.
    pub commit: String,
    /// Kind of the definition (`fn`, `struct`, …).
    pub kind: &'static str,
    /// The definition's name in the current version.
    pub name: String,
}

/// Blame every top-level item of `file`'s committed version, following each item
/// through renames to the commit that last changed its body.
pub fn blame(file: &str) -> Result<Vec<Blame>, Error> {
    let rel = full_name(file)?;
    let commits = log_commits(&rel)?; // newest → oldest
    let Some(head) = commits.first() else {
        return Err(Error::Driver(format!("{file}: no committed history")));
    };

    // Definitions at each commit (newest → oldest). The newest commit's version is
    // the one we blame; older versions tolerate parse failures (treated as absent).
    let mut defs: Vec<Vec<Def>> = Vec::with_capacity(commits.len());
    defs.push(inspect(&show(head, &rel)?)?);
    for c in &commits[1..] {
        defs.push(inspect(&show(c, &rel)?).unwrap_or_default());
    }

    let mut out = Vec::new();
    for head_def in &defs[0] {
        let mut name = head_def.name.clone();
        let mut blame = commits[0].clone(); // last commit with the current body
        let mut i = 0;
        while i + 1 < commits.len() {
            match classify(&match_defs(&defs[i + 1], &defs[i]), &name) {
                Step::Unchanged => {
                    blame = commits[i + 1].clone();
                    i += 1;
                }
                Step::Renamed(from) => {
                    name = from;
                    blame = commits[i + 1].clone();
                    i += 1;
                }
                // Body changed (or first appeared) at the newer commit `i`.
                Step::Changed | Step::Added => break,
            }
        }
        out.push(Blame {
            commit: short(&blame),
            kind: head_def.kind,
            name: head_def.name.clone(),
        });
    }
    Ok(out)
}

/// How the tracked item (by its *new* name) corresponds in the older version.
enum Step {
    Unchanged,
    Renamed(String), // the older name (body unchanged)
    Changed,         // body edited
    Added,           // not present in the older version
}

fn classify(corr: &[Correspondence], name: &str) -> Step {
    for c in corr {
        match c {
            Correspondence::Unchanged { name: n } if n == name => return Step::Unchanged,
            Correspondence::Renamed { from, to } if to == name => {
                return Step::Renamed(from.clone())
            }
            Correspondence::RenamedEdited { to, .. } if to == name => return Step::Changed,
            Correspondence::Modified { name: n } if n == name => return Step::Changed,
            Correspondence::Added { name: n } if n == name => return Step::Added,
            _ => {}
        }
    }
    Step::Added
}

fn short(commit: &str) -> String {
    commit.chars().take(7).collect()
}

/// Run `git <args>` in the current directory, returning stdout text.
fn git(args: &[&str]) -> Result<String, Error> {
    let out = Command::new("git")
        .args(args)
        .output()
        .map_err(|e| Error::Driver(format!("running git: {e}")))?;
    if !out.status.success() {
        return Err(Error::Driver(format!(
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// The file's repo-root-relative path (errors if the file isn't tracked).
fn full_name(file: &str) -> Result<String, Error> {
    git(&["ls-files", "--full-name", "--error-unmatch", "--", file])?
        .lines()
        .next()
        .map(str::to_string)
        .ok_or_else(|| Error::Driver(format!("{file}: not tracked by git")))
}

/// Commits that touched `rel`, newest first.
fn log_commits(rel: &str) -> Result<Vec<String>, Error> {
    Ok(git(&["log", "--format=%H", "--", rel])?
        .lines()
        .map(str::to_string)
        .collect())
}

/// Raw bytes of `rel` at `commit`.
fn show(commit: &str, rel: &str) -> Result<Vec<u8>, Error> {
    let out = Command::new("git")
        .args(["show", &format!("{commit}:{rel}")])
        .output()
        .map_err(|e| Error::Driver(format!("running git show: {e}")))?;
    if !out.status.success() {
        return Err(Error::Driver(format!(
            "git show {commit}:{rel} failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(out.stdout)
}
