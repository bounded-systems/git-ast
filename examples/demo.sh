#!/usr/bin/env bash
# End-to-end demo of the git-ast clean/smudge round-trip.
#
# Builds the binary, creates a throwaway git repo with the filter installed,
# and shows three things:
#   1. messy Rust is stored canonically (clean),
#   2. a pure reformat produces no diff (formatting never enters history),
#   3. a real logic change still shows a clean, minimal diff.
#
# Usage: examples/demo.sh
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$repo_root"

echo "==> building git-ast (release)"
cargo build --release --quiet
bin="$repo_root/target/release/git-ast"

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
cd "$work"
git init -q
"$bin" setup >/dev/null

echo
echo "==> writing deliberately messy Rust"
cat > calc.rs <<'EOF'
fn   add(a:i32,b:i32)->i32{
// Simple addition
       a+b}
fn main(){let x=5;let y =10;
    let sum=add(x,y);println!("Sum: {}",sum);}
EOF
cat calc.rs

echo
echo "==> git add, then show the STORED blob (clean filter output)"
git add calc.rs
git cat-file -p :calc.rs
git commit -qm "add calc.rs"

echo
echo "==> reformat the file wildly, then check git diff"
cat > calc.rs <<'EOF'
fn add(a: i32, b: i32) -> i32 { // Simple addition
        a+b }


fn main() {
        let x = 5;
        let y =     10;
        let sum = add( x , y );
        println!( "Sum: {}" , sum );
}
EOF
if git diff --quiet; then
  echo "(no diff — formatting churn never entered history)"
else
  echo "UNEXPECTED: reformatting produced a diff"; git diff; exit 1
fi

echo
echo "==> make a real change (a + b -> a - b) and show the diff"
sed -i 's/a+b/a-b/' calc.rs
git --no-pager diff
