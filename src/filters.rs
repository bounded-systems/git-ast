//! Clean/smudge filter over Git's long-running `filter-process` protocol.
//!
//! `clean` (on `git add`) parses Rust source and stores its canonical form, so
//! reformatting never reaches history. `smudge` (on `git checkout`) is identity:
//! the stored bytes are already canonical source. Only `*.rs` paths are
//! transformed; anything else passes through untouched.
//!
//! The conversation is the standard one documented in
//! `Documentation/gitattributes.txt`:
//!
//! 1. Handshake — exchange `git-filter-client`/`git-filter-server` + `version=2`.
//! 2. Capabilities — we advertise `clean` and `smudge`.
//! 3. Per blob — read `command`/`pathname` metadata then content, reply with a
//!    status line and the transformed content.

use crate::pktline::{self, Packet};
use crate::{printer, Error};
use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::path::Path;

/// Run the long-running filter process against stdin/stdout.
pub fn run_long_running_filter() -> Result<(), Error> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut input = stdin.lock();
    let mut output = stdout.lock();
    converse(&mut input, &mut output)
}

/// Drive the whole protocol over arbitrary streams (so it is testable without a
/// real Git process).
pub fn converse(input: &mut impl Read, output: &mut impl Write) -> Result<(), Error> {
    handshake(input, output)?;
    capabilities(input, output)?;

    // Process blobs until Git closes the pipe.
    while process_one(input, output)? {}
    Ok(())
}

fn handshake(input: &mut impl Read, output: &mut impl Write) -> Result<(), Error> {
    let intro = pktline::read_until_flush(input)?
        .ok_or_else(|| protocol("client closed during handshake"))?;
    let intro = String::from_utf8_lossy(&intro);
    if !intro.contains("git-filter-client") {
        return Err(protocol("missing git-filter-client welcome"));
    }
    if !intro.contains("version=2") {
        return Err(protocol("client did not offer version=2"));
    }
    pktline::write_text_packet(output, "git-filter-server")?;
    pktline::write_text_packet(output, "version=2")?;
    pktline::write_flush(output)?;
    output.flush()?;
    Ok(())
}

fn capabilities(input: &mut impl Read, output: &mut impl Write) -> Result<(), Error> {
    // Read (and ignore the specifics of) the client's advertised capabilities.
    pktline::read_until_flush(input)?
        .ok_or_else(|| protocol("client closed during capabilities"))?;
    pktline::write_text_packet(output, "capability=clean")?;
    pktline::write_text_packet(output, "capability=smudge")?;
    pktline::write_flush(output)?;
    output.flush()?;
    Ok(())
}

/// Handle a single blob request. Returns `Ok(false)` when the client has closed
/// the stream (no more work), `Ok(true)` after a blob was processed.
fn process_one(input: &mut impl Read, output: &mut impl Write) -> Result<bool, Error> {
    // Metadata section: key=value lines terminated by a flush. EOF here is the
    // normal shutdown signal.
    let meta = match read_meta(input)? {
        Some(meta) => meta,
        None => return Ok(false),
    };
    let command = meta.get("command").map(String::as_str).unwrap_or_default();
    let pathname = meta.get("pathname").cloned().unwrap_or_default();

    // Content section.
    let content =
        pktline::read_until_flush(input)?.ok_or_else(|| protocol("client closed mid-content"))?;

    match transform(command, &pathname, &content) {
        Ok(out) => {
            pktline::write_text_packet(output, "status=success")?;
            pktline::write_flush(output)?;
            pktline::write_content(output, &out)?;
            // Trailing empty status list: leaves status=success in effect.
            pktline::write_flush(output)?;
        }
        Err(e) => {
            // Report the blob as failed; Git aborts the add/checkout for it.
            eprintln!("git-ast: {pathname}: {e}");
            pktline::write_text_packet(output, "status=error")?;
            pktline::write_flush(output)?;
        }
    }
    output.flush()?;
    Ok(true)
}

/// Apply the requested transform. `clean` canonicalizes Rust; `smudge` is
/// identity. Non-Rust paths pass through unchanged in both directions.
fn transform(command: &str, pathname: &str, content: &[u8]) -> Result<Vec<u8>, Error> {
    let is_rust = Path::new(pathname).extension().is_some_and(|e| e == "rs");
    match command {
        "clean" if is_rust => printer::canonicalize(content),
        "smudge" | "clean" => Ok(content.to_vec()),
        other => Err(Error::Driver(format!("unknown filter command `{other}`"))),
    }
}

/// Read the metadata section into key/value pairs. Returns `None` at clean EOF.
fn read_meta(input: &mut impl Read) -> Result<Option<HashMap<String, String>>, Error> {
    let mut map = HashMap::new();
    let mut saw_any = false;
    loop {
        match pktline::read_packet(input)? {
            None if !saw_any => return Ok(None),
            None => return Err(protocol("EOF in metadata section")),
            Some(Packet::Flush) => return Ok(Some(map)),
            Some(Packet::Data(d)) => {
                saw_any = true;
                let line = String::from_utf8_lossy(&d);
                let line = line.trim_end_matches('\n');
                if let Some((k, v)) = line.split_once('=') {
                    map.insert(k.to_string(), v.to_string());
                }
            }
        }
    }
}

fn protocol(msg: &str) -> Error {
    Error::Driver(format!("filter protocol: {msg}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pktline::write_text_packet;

    /// Build a client-side request stream: handshake, capabilities, then one
    /// blob with the given command/path/content.
    fn client_stream(command: &str, pathname: &str, content: &[u8]) -> Vec<u8> {
        let mut w = Vec::new();
        write_text_packet(&mut w, "git-filter-client").unwrap();
        write_text_packet(&mut w, "version=2").unwrap();
        pktline::write_flush(&mut w).unwrap();
        write_text_packet(&mut w, "capability=clean").unwrap();
        write_text_packet(&mut w, "capability=smudge").unwrap();
        pktline::write_flush(&mut w).unwrap();
        write_text_packet(&mut w, &format!("command={command}")).unwrap();
        write_text_packet(&mut w, &format!("pathname={pathname}")).unwrap();
        pktline::write_flush(&mut w).unwrap();
        pktline::write_content(&mut w, content).unwrap();
        w
    }

    /// Pull the content of the (single) blob response back out of the server's
    /// reply stream, skipping the handshake/capability/status sections.
    fn response_content(reply: &[u8]) -> Vec<u8> {
        let mut r = reply;
        pktline::read_until_flush(&mut r).unwrap(); // server handshake
        pktline::read_until_flush(&mut r).unwrap(); // server capabilities
        pktline::read_until_flush(&mut r).unwrap(); // status list
        pktline::read_until_flush(&mut r).unwrap().unwrap() // content
    }

    #[test]
    fn clean_canonicalizes_rust() {
        let req = client_stream("clean", "a.rs", b"fn f()->i32{1+2}");
        let mut out = Vec::new();
        converse(&mut &req[..], &mut out).unwrap();
        assert_eq!(response_content(&out), b"fn f() -> i32 {\n    1 + 2\n}\n");
    }

    #[test]
    fn smudge_is_identity() {
        let canonical = b"fn f() -> i32 {\n    1 + 2\n}\n";
        let req = client_stream("smudge", "a.rs", canonical);
        let mut out = Vec::new();
        converse(&mut &req[..], &mut out).unwrap();
        assert_eq!(response_content(&out), canonical);
    }

    #[test]
    fn non_rust_passes_through_clean() {
        let req = client_stream("clean", "notes.txt", b"  unchanged  ");
        let mut out = Vec::new();
        converse(&mut &req[..], &mut out).unwrap();
        assert_eq!(response_content(&out), b"  unchanged  ");
    }

    #[test]
    fn clean_reports_error_on_unparseable_rust() {
        let req = client_stream("clean", "bad.rs", b"fn main( {");
        let mut out = Vec::new();
        converse(&mut &req[..], &mut out).unwrap();
        // Status section should carry status=error and no content follows.
        let mut r = &out[..];
        pktline::read_until_flush(&mut r).unwrap(); // handshake
        pktline::read_until_flush(&mut r).unwrap(); // capabilities
        let status = pktline::read_until_flush(&mut r).unwrap().unwrap();
        assert_eq!(String::from_utf8_lossy(&status).trim_end(), "status=error");
    }
}
