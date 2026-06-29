//! Executable specification of git-ast's behavioural claims.
//!
//! Each scenario in `tests/features/claims.feature` drives **real git** with the
//! built `git-ast` binary installed as the clean/smudge filter, so the README's
//! claims (reformatting is invisible, determinism, fail-closed, passthrough,
//! round-trip) are verified end to end rather than asserted in prose.
//!
//! Run with `cargo test --test claims`.

use std::path::Path;
use std::process::Command;

use cucumber::gherkin::Step;
use cucumber::{given, then, when, World};
use tempfile::TempDir;

#[derive(Debug, Default, World)]
struct AstWorld {
    repo: Option<TempDir>,
    last_add_code: i32,
    last_merge_code: i32,
}

impl AstWorld {
    fn dir(&self) -> &Path {
        self.repo
            .as_ref()
            .expect("repository not initialized")
            .path()
    }

    /// Run a git command in the repo; return (exit code, stdout).
    fn git(&self, args: &[&str]) -> (i32, String) {
        let out = Command::new("git")
            .args(args)
            .current_dir(self.dir())
            .output()
            .expect("failed to run git");
        (
            out.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&out.stdout).into_owned(),
        )
    }

    fn write(&self, name: &str, content: &str) {
        std::fs::write(self.dir().join(name), content).expect("failed to write file");
    }

    fn stored_blob(&self, name: &str) -> String {
        self.git(&["cat-file", "-p", &format!(":{name}")]).1
    }
}

#[given("a repository with git-ast installed")]
async fn install(world: &mut AstWorld) {
    let repo = tempfile::tempdir().expect("tempdir");
    let dir = repo.path().to_path_buf();
    let run = |args: &[&str]| {
        Command::new("git")
            .args(args)
            .current_dir(&dir)
            .status()
            .expect("failed to run git");
    };
    run(&["init", "-q"]);
    run(&["config", "user.email", "test@example.com"]);
    run(&["config", "user.name", "git-ast test"]);
    let bin = env!("CARGO_BIN_EXE_git-ast");
    run(&[
        "config",
        "filter.git-ast.process",
        &format!("{bin} filter-process"),
    ]);
    run(&["config", "filter.git-ast.required", "true"]);
    // Structural merge driver (JSON).
    run(&["config", "merge.git-ast.name", "git-ast structural merge"]);
    run(&[
        "config",
        "merge.git-ast.driver",
        &format!("{bin} merge-driver %O %A %B %L %P"),
    ]);
    std::fs::write(
        dir.join(".gitattributes"),
        "*.rs filter=git-ast\n*.json filter=git-ast\n*.json merge=git-ast\n",
    )
    .expect("write attrs");
    world.repo = Some(repo);
}

#[when(expr = "I stage {string} containing:")]
async fn stage_doc(world: &mut AstWorld, name: String, step: &Step) {
    let body = step.docstring.clone().unwrap_or_default();
    world.write(&name, &body);
    world.last_add_code = world.git(&["add", &name]).0;
}

#[when(expr = "I stage {string} containing {string}")]
async fn stage_inline(world: &mut AstWorld, name: String, content: String) {
    world.write(&name, &content);
    world.last_add_code = world.git(&["add", &name]).0;
}

#[when("I commit")]
async fn commit(world: &mut AstWorld) {
    world.git(&["commit", "-qm", "snapshot"]);
}

#[when(expr = "I overwrite {string} with:")]
async fn overwrite(world: &mut AstWorld, name: String, step: &Step) {
    world.write(&name, &step.docstring.clone().unwrap_or_default());
}

#[when(expr = "I check out {string} fresh")]
async fn checkout_fresh(world: &mut AstWorld, name: String) {
    std::fs::remove_file(world.dir().join(&name)).ok();
    world.git(&["checkout", "--", &name]);
}

#[then(expr = "the stored blobs for {string} and {string} are identical")]
async fn blobs_identical(world: &mut AstWorld, a: String, b: String) {
    assert_eq!(world.stored_blob(&a), world.stored_blob(&b));
}

#[then(expr = "the stored blob for {string} is {string}")]
async fn blob_is_inline(world: &mut AstWorld, name: String, want: String) {
    // Exact compare: proves non-Rust passthrough preserves bytes verbatim.
    assert_eq!(world.stored_blob(&name), want);
}

#[then(expr = "the working file {string} is:")]
async fn working_is(world: &mut AstWorld, name: String, step: &Step) {
    let want = step.docstring.clone().unwrap_or_default();
    let got = std::fs::read_to_string(world.dir().join(&name)).expect("read working file");
    // Compare canonical content; exact leading/trailing newline bytes are
    // guarded separately by the printer unit tests.
    assert_eq!(got.trim_matches('\n'), want.trim_matches('\n'));
}

#[then(expr = "{string} shows no diff")]
async fn shows_no_diff(world: &mut AstWorld, name: String) {
    let (code, _) = world.git(&["diff", "--quiet", "--", &name]);
    assert_eq!(code, 0, "expected no diff for {name}");
}

#[then(expr = "{string} shows a diff")]
async fn shows_a_diff(world: &mut AstWorld, name: String) {
    let (code, _) = world.git(&["diff", "--quiet", "--", &name]);
    assert_ne!(code, 0, "expected a diff for {name}");
}

#[then(expr = "staging {string} containing {string} is rejected")]
async fn staging_rejected(world: &mut AstWorld, name: String, content: String) {
    world.write(&name, &content);
    let (code, _) = world.git(&["add", &name]);
    assert_ne!(code, 0, "expected `git add {name}` to fail (fail-closed)");
}

#[when(expr = "I branch {string}")]
async fn branch(world: &mut AstWorld, name: String) {
    world.git(&["checkout", "-q", "-b", &name]);
}

#[when("I check out the original branch")]
async fn checkout_previous(world: &mut AstWorld) {
    // `-` returns to the previously checked-out branch — avoids hard-coding
    // whether `git init` produced `main` or `master`.
    world.git(&["checkout", "-q", "-"]);
}

#[when(expr = "I merge {string}")]
async fn merge_branch(world: &mut AstWorld, name: String) {
    let (code, _) = world.git(&["merge", "--no-edit", &name]);
    world.last_merge_code = code;
}

#[then("the merge succeeds")]
async fn merge_succeeds(world: &mut AstWorld) {
    assert_eq!(world.last_merge_code, 0, "expected a clean merge");
}

#[then("the merge conflicts")]
async fn merge_conflicts(world: &mut AstWorld) {
    assert_ne!(world.last_merge_code, 0, "expected a merge conflict");
}

#[tokio::main]
async fn main() {
    AstWorld::cucumber().run_and_exit("tests/features").await;
}
