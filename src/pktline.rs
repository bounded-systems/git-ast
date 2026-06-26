//! Minimal pkt-line codec for Git's long-running filter protocol.
//!
//! Git frames the `filter-process` conversation in *pkt-lines*: a 4-byte
//! lowercase-hex length prefix (counting the 4 prefix bytes themselves)
//! followed by that many bytes of payload. The special length `0000` is a
//! *flush* packet, used as a delimiter. See `gitprotocol-common` and
//! `Documentation/gitattributes.txt` ("Long Running Filter Process").
//!
//! This module is transport-only: it reads and writes frames and knows nothing
//! about clean/smudge. [`crate::filters`] drives the conversation.

use std::io::{self, Read, Write};

/// Largest payload Git accepts in a single pkt-line (65520 total minus the
/// 4-byte length prefix).
pub const MAX_PAYLOAD: usize = 65516;

/// One frame read from the wire.
#[derive(Debug, PartialEq, Eq)]
pub enum Packet {
    /// A data packet and its payload (newline included, if any).
    Data(Vec<u8>),
    /// A flush packet (`0000`) — the delimiter between sections.
    Flush,
}

/// Read a single pkt-line frame.
///
/// Returns `Ok(None)` only at clean end-of-stream (Git closed the pipe), which
/// the caller treats as "shut down".
pub fn read_packet(reader: &mut impl Read) -> io::Result<Option<Packet>> {
    let mut len_buf = [0u8; 4];
    if !read_exact_or_eof(reader, &mut len_buf)? {
        return Ok(None);
    }
    let len = parse_hex4(&len_buf)?;
    if len == 0 {
        return Ok(Some(Packet::Flush));
    }
    if len < 4 {
        return Err(invalid(format!("pkt-line length {len} < 4")));
    }
    let mut payload = vec![0u8; len - 4];
    reader.read_exact(&mut payload)?;
    Ok(Some(Packet::Data(payload)))
}

/// Read a section of data packets up to the next flush, concatenating payloads.
///
/// Returns `Ok(None)` at end-of-stream before any packet.
pub fn read_until_flush(reader: &mut impl Read) -> io::Result<Option<Vec<u8>>> {
    let mut buf = Vec::new();
    let mut saw_any = false;
    loop {
        match read_packet(reader)? {
            None if !saw_any => return Ok(None),
            None => return Err(invalid("unexpected EOF before flush")),
            Some(Packet::Flush) => return Ok(Some(buf)),
            Some(Packet::Data(d)) => {
                saw_any = true;
                buf.extend_from_slice(&d);
            }
        }
    }
}

/// Write a single data packet. `payload` must be <= [`MAX_PAYLOAD`].
pub fn write_packet(writer: &mut impl Write, payload: &[u8]) -> io::Result<()> {
    debug_assert!(payload.len() <= MAX_PAYLOAD);
    let len = payload.len() + 4;
    write!(writer, "{len:04x}")?;
    writer.write_all(payload)?;
    Ok(())
}

/// Write a `key=value` text packet (a trailing newline is appended, as Git
/// expects for metadata lines).
pub fn write_text_packet(writer: &mut impl Write, line: &str) -> io::Result<()> {
    let mut payload = line.as_bytes().to_vec();
    payload.push(b'\n');
    write_packet(writer, &payload)
}

/// Write a flush packet (`0000`).
pub fn write_flush(writer: &mut impl Write) -> io::Result<()> {
    writer.write_all(b"0000")
}

/// Write a payload of arbitrary size as a sequence of data packets, chunked to
/// [`MAX_PAYLOAD`], followed by a flush.
pub fn write_content(writer: &mut impl Write, content: &[u8]) -> io::Result<()> {
    for chunk in content.chunks(MAX_PAYLOAD).filter(|c| !c.is_empty()) {
        write_packet(writer, chunk)?;
    }
    write_flush(writer)
}

fn parse_hex4(buf: &[u8; 4]) -> io::Result<usize> {
    let s = std::str::from_utf8(buf).map_err(|_| invalid("non-ascii pkt-line length"))?;
    usize::from_str_radix(s, 16).map_err(|_| invalid(format!("bad pkt-line length {s:?}")))
}

/// Read exactly `buf.len()` bytes, or report clean EOF if nothing was read.
fn read_exact_or_eof(reader: &mut impl Read, buf: &mut [u8]) -> io::Result<bool> {
    let mut filled = 0;
    while filled < buf.len() {
        match reader.read(&mut buf[filled..])? {
            0 if filled == 0 => return Ok(false),
            0 => return Err(invalid("EOF in middle of pkt-line length")),
            n => filled += n,
        }
    }
    Ok(true)
}

fn invalid(msg: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrips_a_text_packet() {
        let mut buf = Vec::new();
        write_text_packet(&mut buf, "version=2").unwrap();
        assert_eq!(buf, b"000eversion=2\n");
        let pkt = read_packet(&mut &buf[..]).unwrap().unwrap();
        assert_eq!(pkt, Packet::Data(b"version=2\n".to_vec()));
    }

    #[test]
    fn flush_roundtrips() {
        let mut buf = Vec::new();
        write_flush(&mut buf).unwrap();
        assert_eq!(buf, b"0000");
        assert_eq!(read_packet(&mut &buf[..]).unwrap().unwrap(), Packet::Flush);
    }

    #[test]
    fn reads_a_section_until_flush() {
        let mut wire = Vec::new();
        write_packet(&mut wire, b"hello ").unwrap();
        write_packet(&mut wire, b"world").unwrap();
        write_flush(&mut wire).unwrap();
        let got = read_until_flush(&mut &wire[..]).unwrap().unwrap();
        assert_eq!(got, b"hello world");
    }

    #[test]
    fn chunks_large_content() {
        let big = vec![b'x'; MAX_PAYLOAD * 2 + 7];
        let mut wire = Vec::new();
        write_content(&mut wire, &big).unwrap();
        let got = read_until_flush(&mut &wire[..]).unwrap().unwrap();
        assert_eq!(got, big);
    }

    #[test]
    fn eof_before_any_packet_is_none() {
        let empty: &[u8] = b"";
        assert!(read_packet(&mut &empty[..]).unwrap().is_none());
    }
}
