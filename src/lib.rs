//! # git-ast: Language-Aware Git Extensions
//!
//! `git-ast` aims to extend Git with language-aware behaviour: source is parsed
//! into a syntax tree (AST/CST) on `git add` and pretty-printed back to canonical
//! source on `git checkout`, so that diffs and merges can operate on structure
//! rather than text lines.
//!
//! ## Status
//!
//! **Working clean/smudge round-trip for two languages.** The `clean` filter
//! canonicalizes Rust (a documented subset, via Tree-sitter — see [`printer`])
//! and JSON (via `serde_json` — see [`json`]), driven over Git's real
//! `filter-process` pkt-line protocol ([`pktline`], [`filters`]) — so `git add`/
//! `git checkout` normalize formatting end to end. Both paths are fail-closed:
//! unparseable input rejects the commit rather than storing junk. The diff and
//! merge drivers ([`drivers`]) remain placeholders: making those structural
//! depends on stable node identity, which is out of scope (see
//! `docs/planning/scope.md`).
//!
//! ## Integration points
//!
//! `git-ast` plugs into Git via `.gitattributes` and git config:
//!
//! - **Clean/smudge filter** (`git-ast filter-process`) — see [`filters`].
//! - **Diff driver** (`git-ast diff-driver`) — see [`drivers`].
//! - **Merge driver** (`git-ast merge-driver`) — see [`drivers`].
//!
//! Configuration is read via [`config`].

pub mod config;
pub mod drivers;
pub mod filters;
pub mod json;
pub mod merge;
pub mod pktline;
pub mod printer;
pub mod setup;

use std::fmt;

/// Shared error type for git-ast subcommands.
#[derive(Debug)]
pub enum Error {
    /// An underlying I/O failure.
    Io(std::io::Error),
    /// Configuration could not be read or interpreted.
    Config(String),
    /// Source could not be parsed into a tree.
    Parsing(String),
    /// A tree could not be serialized.
    Serialization(String),
    /// Source could not be generated from a tree.
    Generation(String),
    /// A diff or merge driver failed.
    Driver(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "io error: {e}"),
            Error::Config(m) => write!(f, "config error: {m}"),
            Error::Parsing(m) => write!(f, "parse error: {m}"),
            Error::Serialization(m) => write!(f, "serialization error: {m}"),
            Error::Generation(m) => write!(f, "generation error: {m}"),
            Error::Driver(m) => write!(f, "driver error: {m}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display_is_human_readable() {
        let e = Error::Config("missing driver".to_string());
        assert_eq!(e.to_string(), "config error: missing driver");
    }

    #[test]
    fn io_error_converts_and_chains_source() {
        let io = std::io::Error::new(std::io::ErrorKind::NotFound, "nope");
        let e: Error = io.into();
        assert!(matches!(e, Error::Io(_)));
        assert!(std::error::Error::source(&e).is_some());
    }
}
