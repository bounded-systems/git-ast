//! End-to-end test of `git-ast blame` against real git history — the refactor-
//! aware property: a function is followed through a rename.

use std::fs;
use std::path::Path;
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_git-ast");

fn git(repo: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .args(args)
        .current_dir(repo)
        .status()
        .expect("git")
        .success();
    assert!(ok, "git {args:?} failed");
}

fn commit(repo: &Path, file: &str, content: &str, msg: &str) -> String {
    fs::write(repo.join(file), content).unwrap();
    git(repo, &["add", file]);
    git(repo, &["commit", "-qm", msg]);
    let out = Command::new("git")
        .args(["rev-parse", "--short=7", "HEAD"])
        .current_dir(repo)
        .output()
        .unwrap();
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

#[test]
fn blame_follows_a_function_through_a_rename() {
    let repo = Path::new(env!("CARGO_TARGET_TMPDIR")).join("blame_repo");
    let _ = fs::remove_dir_all(&repo);
    fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-q"]);
    git(&repo, &["config", "user.email", "t@test.local"]);
    git(&repo, &["config", "user.name", "t"]);

    // c1: three functions.
    let c1 = commit(
        &repo,
        "m.rs",
        "fn alpha() -> i32 { 1 }\nfn beta() -> i32 { 2 }\nfn gamma() -> i32 { 3 }\n",
        "c1",
    );
    // c2: edit beta's body.
    let c2 = commit(
        &repo,
        "m.rs",
        "fn alpha() -> i32 { 1 }\nfn beta() -> i32 { 99 }\nfn gamma() -> i32 { 3 }\n",
        "c2",
    );
    // c3: rename gamma -> gamma2 (body unchanged), and add delta.
    let c3 = commit(
        &repo,
        "m.rs",
        "fn alpha() -> i32 { 1 }\nfn beta() -> i32 { 99 }\nfn gamma2() -> i32 { 3 }\nfn delta() -> i32 { 4 }\n",
        "c3",
    );

    let out = Command::new(BIN)
        .arg("blame")
        .arg("m.rs")
        .current_dir(&repo)
        .output()
        .expect("git-ast blame");
    assert!(
        out.status.success(),
        "blame failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let out = String::from_utf8(out.stdout).unwrap();

    // alpha never changed → c1; beta edited → c2; delta added → c3;
    // gamma2 was renamed in c3 but its BODY is from c1 → blame sees through to c1.
    assert!(out.contains(&format!("{c1}  fn     alpha")), "got:\n{out}");
    assert!(out.contains(&format!("{c2}  fn     beta")), "got:\n{out}");
    assert!(
        out.contains(&format!("{c1}  fn     gamma2")),
        "refactor-aware: rename must be seen through; got:\n{out}"
    );
    assert!(out.contains(&format!("{c3}  fn     delta")), "got:\n{out}");
}
