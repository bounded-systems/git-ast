//! End-to-end test of the `git-ast match` verb (node identity) via the binary.

use std::fs;
use std::path::Path;
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_git-ast");

fn run_match(old: &str, new: &str) -> String {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("match_cli");
    fs::create_dir_all(&dir).unwrap();
    let old_path = dir.join("old.rs");
    let new_path = dir.join("new.rs");
    fs::write(&old_path, old).unwrap();
    fs::write(&new_path, new).unwrap();
    let out = Command::new(BIN)
        .arg("match")
        .arg(&old_path)
        .arg(&new_path)
        .output()
        .expect("run git-ast match");
    assert!(
        out.status.success(),
        "match failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn match_verb_tracks_all_correspondences() {
    let old = "fn keep()->i32{1}\n\
               fn edit()->i32{2}\n\
               fn renameMe(x: i32)->i32{x+1}\n\
               fn parseConfig(s: i32)->i32{let v = s + 1; v * 2}\n\
               fn gone()->i32{4}";
    let new = "fn keep()->i32{1}\n\
               fn edit()->i32{99}\n\
               fn renamed(x: i32)->i32{x+1}\n\
               fn loadSettings(s: i32)->i32{let v = s + 1; v * 3}\n\
               fn fresh(a: i32, b: i32)->i32{let p = a + b; helper(p)}";
    let out = run_match(old, new);
    assert!(out.contains("unchanged  keep"), "got:\n{out}");
    assert!(out.contains("modified   edit"), "got:\n{out}");
    assert!(
        out.contains("renamed    renameMe -> renamed"),
        "got:\n{out}"
    );
    // Fuzzy: renamed AND edited.
    assert!(
        out.contains("renamed+   parseConfig -> loadSettings"),
        "got:\n{out}"
    );
    assert!(out.contains("removed    gone"), "got:\n{out}");
    assert!(out.contains("added      fresh"), "got:\n{out}");
}

#[test]
fn match_script_prints_a_statement_edit_script() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("match_script");
    fs::create_dir_all(&dir).unwrap();
    let old = dir.join("old.rs");
    let new = dir.join("new.rs");
    fs::write(
        &old,
        "fn compute(a: i32, b: i32) -> i32 { let x = a + 1; let y = b + 2; let tmp = 0; x * y }",
    )
    .unwrap();
    fs::write(
        &new,
        "fn compute(a: i32, b: i32) -> i32 { let y = b + 2; let x = a + 1; log(x); x + y }",
    )
    .unwrap();
    let out = Command::new(BIN)
        .arg("match")
        .arg("--script")
        .arg(&old)
        .arg(&new)
        .output()
        .expect("run git-ast match --script");
    assert!(out.status.success());
    let out = String::from_utf8(out.stdout).unwrap();
    assert!(out.contains("modified   compute"), "got:\n{out}");
    assert!(out.contains("moved      let x = a + 1;"), "got:\n{out}");
    assert!(out.contains("inserted   log(x);"), "got:\n{out}");
    assert!(out.contains("changed    x * y  ->  x + y"), "got:\n{out}");
    assert!(out.contains("deleted    let tmp = 0;"), "got:\n{out}");
}
