//! `git-ast` command-line entry point.
//!
//! Dispatches the Git integration subcommands. `setup` and `filter-process`
//! implement the working clean/smudge round-trip; `diff-driver`/`merge-driver`
//! remain placeholders (they await stable node identity — see `docs/`).

use std::io::Read;
use std::process::ExitCode;

use git_ast::{drivers, filters, printer, setup, Error};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some((cmd, rest)) = args.split_first() else {
        print_help();
        return ExitCode::SUCCESS;
    };

    let result = match cmd.as_str() {
        "setup" => setup::run().map(|()| 0u8),
        "inspect" => run_inspect(rest),
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
    let source = match args.first() {
        Some(path) => std::fs::read(path)?,
        None => {
            let mut buf = Vec::new();
            std::io::stdin().read_to_end(&mut buf)?;
            buf
        }
    };
    for def in printer::inspect(&source)? {
        println!("{} {} {}", def.kind, def.name, def.content_hash);
    }
    Ok(0)
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
