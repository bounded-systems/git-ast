//! `git-ast` command-line entry point.
//!
//! Dispatches the Git integration subcommands. `setup` and `filter-process`
//! implement the working clean/smudge round-trip; `diff-driver`/`merge-driver`
//! remain placeholders (they await stable node identity — see `docs/`).

use std::process::ExitCode;

use git_ast::{drivers, filters, setup};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some((cmd, rest)) = args.split_first() else {
        print_help();
        return ExitCode::SUCCESS;
    };

    let result = match cmd.as_str() {
        "setup" => setup::run().map(|()| 0u8),
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

fn print_help() {
    eprintln!(
        "git-ast — language-aware Git\n\
         \n\
         USAGE:\n    \
         git-ast <SUBCOMMAND>\n\
         \n\
         SUBCOMMANDS:\n    \
         setup             Enable the *.rs clean/smudge filter in this repo\n    \
         filter-process    Clean/smudge long-running filter (canonicalizes Rust)\n    \
         diff-driver       Git diff driver (placeholder)\n    \
         merge-driver      Git merge driver (placeholder)\n    \
         --version, -V     Print version\n    \
         --help, -h        Print this help\n\
         \n\
         The clean/smudge round-trip works for a documented Rust subset and is\n\
         fail-closed outside it. Structural diff/merge await stable node identity;\n\
         see docs/ for the design and scope."
    );
}
