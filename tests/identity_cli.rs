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
fn match_verb_tracks_rename_modify_add_remove() {
    let old = "fn keep()->i32{1}\nfn edit()->i32{2}\nfn renameMe()->i32{3}\nfn drop()->i32{4}";
    let new = "fn keep()->i32{1}\nfn edit()->i32{99}\nfn renamed()->i32{3}\nfn fresh()->i32{5}";
    let out = run_match(old, new);
    assert!(out.contains("unchanged  keep"), "got:\n{out}");
    assert!(out.contains("modified   edit"), "got:\n{out}");
    assert!(
        out.contains("renamed    renameMe -> renamed"),
        "got:\n{out}"
    );
    assert!(out.contains("removed    drop"), "got:\n{out}");
    assert!(out.contains("added      fresh"), "got:\n{out}");
}
