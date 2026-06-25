//! Clean/smudge filter.
//!
//! `clean` turns source text into a serialized tree on `git add`; `smudge` turns
//! it back into source on checkout. Both are placeholders: `clean` prefixes the
//! content with a marker and `smudge` strips it. A real implementation would
//! parse with Tree-sitter and pretty-print deterministically.

use crate::Error;
use std::io::Read;

/// Marker that the placeholder clean/smudge round-trip uses to stand in for a
/// serialized tree.
const SERIALIZED_MARKER: &[u8] = b"SERIALIZED:";

/// Run the long-running filter process.
///
/// Placeholder: reads stdin to EOF and reports the byte count instead of
/// speaking Git's pkt-line filter protocol.
pub fn run_long_running_filter() -> Result<(), Error> {
    eprintln!("[filter] long-running filter process (placeholder)");
    let mut buffer = Vec::new();
    std::io::stdin().read_to_end(&mut buffer)?;
    eprintln!("[filter] read {} bytes (no-op)", buffer.len());
    Ok(())
}

/// `clean`: source text -> serialized tree.
///
/// Placeholder: prefixes the content with [`SERIALIZED_MARKER`].
#[allow(dead_code)]
fn perform_clean(input: &[u8], pathname: &str) -> Result<Vec<u8>, Error> {
    eprintln!("[filter] clean {pathname}");
    let mut out = SERIALIZED_MARKER.to_vec();
    out.extend_from_slice(input);
    Ok(out)
}

/// `smudge`: serialized tree -> source text.
///
/// Placeholder: strips [`SERIALIZED_MARKER`] if present, otherwise passes through.
#[allow(dead_code)]
fn perform_smudge(input: &[u8], pathname: &str) -> Result<Vec<u8>, Error> {
    eprintln!("[filter] smudge {pathname}");
    match input.strip_prefix(SERIALIZED_MARKER) {
        Some(rest) => Ok(rest.to_vec()),
        None => Ok(input.to_vec()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_then_smudge_round_trips() {
        let src = b"fn main() {}\n";
        let cleaned = perform_clean(src, "a.rs").unwrap();
        assert!(cleaned.starts_with(SERIALIZED_MARKER));
        let smudged = perform_smudge(&cleaned, "a.rs").unwrap();
        assert_eq!(smudged, src);
    }

    #[test]
    fn smudge_passes_through_unmarked_content() {
        let raw = b"plain text";
        assert_eq!(perform_smudge(raw, "a.rs").unwrap(), raw);
    }
}
