//! `git-ast` command-line entry point.
//!
//! Dispatches the Git integration subcommands. `setup` and `filter-process`
//! implement the working clean/smudge round-trip; `diff-driver`/`merge-driver`
//! remain placeholders (they await stable node identity — see `docs/`).

use std::io::Read;
use std::process::ExitCode;

use git_ast::{blame, drivers, filters, html, identity, printer, setup, Error};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some((cmd, rest)) = args.split_first() else {
        print_help();
        return ExitCode::SUCCESS;
    };

    let result = match cmd.as_str() {
        "setup" => setup::run().map(|()| 0u8),
        "inspect" => run_inspect(rest),
        "match" => run_match(rest),
        "blame" => run_blame(rest),
        "filter-process" => filters::run_long_running_filter().map(|()| 0u8),
        "diff-driver" => drivers::run_diff_driver(rest).map(|()| 0u8),
        "merge-driver" => drivers::run_merge_driver(rest).map(|()| 0u8),
        "--version" | "-V" => {
            println!("git-ast {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        "--help" | "-h" => {
            print_help();
            return ExitCode::SUCCESS;
        }
        other => {
            eprintln!("git-ast: unknown subcommand '{other}'");
            print_help();
            return ExitCode::from(2);
        }
    };

    match result {
        Ok(code) => ExitCode::from(code),
        Err(e) => {
            eprintln!("git-ast: {e}");
            ExitCode::FAILURE
        }
    }
}

/// The `inspect` read verb: list top-level definitions with a
/// formatting-invariant content hash. Reads a file argument, or stdin.
fn run_inspect(args: &[String]) -> Result<u8, Error> {
    let path = args.first().map(String::as_str);
    let source = match path {
        Some(p) => std::fs::read(p)?,
        None => {
            let mut buf = Vec::new();
            std::io::stdin().read_to_end(&mut buf)?;
            buf
        }
    };
    let ext = path
        .and_then(|p| std::path::Path::new(p).extension())
        .and_then(|e| e.to_str());
    let defs = match ext {
        Some("html") | Some("htm") => html::inspect(&source)?,
        _ => printer::inspect(&source)?,
    };
    for def in defs {
        println!("{} {} {}", def.kind, def.name, def.content_hash);
    }
    Ok(0)
}

/// The `match` read verb: correspond top-level definitions across two versions
/// (`git-ast match [--script] <old> <new>`), reporting each as unchanged / renamed
/// / modified / added / removed via content-addressed identity. With `--script`,
/// each changed function also gets a statement-level structural edit script.
fn run_match(args: &[String]) -> Result<u8, Error> {
    let (script, rest) = match args.split_first() {
        Some((flag, tail)) if flag == "--script" => (true, tail),
        _ => (false, args),
    };
    let (Some(old_path), Some(new_path)) = (rest.first(), rest.get(1)) else {
        return Err(Error::Config(
            "match expects: git-ast match [--script] <old> <new>".to_string(),
        ));
    };
    let old_src = std::fs::read(old_path)?;
    let new_src = std::fs::read(new_path)?;
    let ext = std::path::Path::new(old_path)
        .extension()
        .and_then(|e| e.to_str());
    let (old, new) = match ext {
        Some("html") | Some("htm") => (html::inspect(&old_src)?, html::inspect(&new_src)?),
        _ => (printer::inspect(&old_src)?, printer::inspect(&new_src)?),
    };

    for c in identity::match_defs(&old, &new) {
        print!("{}", identity::render_correspondence(&c));
        // With --script, show what changed *inside* a matched-but-changed function.
        if script {
            if let Some((from, to)) = changed_fn_names(&c) {
                if let (Some(os), Some(ns)) = (
                    printer::function_statements(&old_src, from),
                    printer::function_statements(&new_src, to),
                ) {
                    for op in identity::edit_script(&os, &ns) {
                        print!("{}", identity::render_edit_op(&op));
                    }
                }
            }
        }
    }
    Ok(0)
}

/// The `blame` verb: refactor-aware, per-definition blame
/// (`git-ast blame <file>`). For each top-level item, prints the commit that last
/// changed it — following it through renames.
fn run_blame(args: &[String]) -> Result<u8, Error> {
    let Some(file) = args.first() else {
        return Err(Error::Config(
            "blame expects a file: git-ast blame <file>".to_string(),
        ));
    };
    for b in blame::blame(file)? {
        println!("{}  {:<6} {}", b.commit, b.kind, b.name);
    }
    Ok(0)
}

/// The (old, new) function names for a correspondence that changed a body, or
/// `None` for unchanged/renamed-only/added/removed.
fn changed_fn_names(c: &identity::Correspondence) -> Option<(&str, &str)> {
    match c {
        identity::Correspondence::Modified { name } => Some((name, name)),
        identity::Correspondence::RenamedEdited { from, to, .. } => Some((from, to)),
        _ => None,
    }
}

fn print_help() {
    eprintln!(
        "git-ast — language-aware Git\n\
         \n\
         USAGE:\n    \
         git-ast <SUBCOMMAND>\n\
         \n\
         SUBCOMMANDS:\n    \
         setup             Enable the *.rs and *.json clean/smudge filter here\n    \
         inspect [FILE]    List top-level defs with a formatting-invariant hash\n    \
         match OLD NEW     Correspond defs across two versions (rename/move/edit)\n    \
         blame FILE        Per-def blame, following items through renames\n    \
         filter-process    Clean/smudge long-running filter (Rust + JSON)\n    \
         diff-driver       Structural diff (JSON); text diff otherwise\n    \
         merge-driver      Structural 3-way merge (JSON)\n    \
         --version, -V     Print version\n    \
         --help, -h        Print this help\n\
         \n\
         The clean/smudge round-trip canonicalizes JSON and a documented Rust\n\
         subset; structural merge & diff are real for JSON. Refactor-aware history\n\
         (node identity) is the remaining frontier; see docs/ for the design."
    );
}
