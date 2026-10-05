//! Bounded line reader for the IPC server (task I1).
//!
//! A peer must not be able to make the service buffer an unbounded amount of
//! memory by never sending a newline.
use std::io::{self, BufRead};

/// Fixed message for invalid UTF-8, so it never echoes request bytes.
pub const INVALID_UTF8_MESSAGE: &str = "request line is not valid UTF-8";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineRead {
    /// Clean end of stream, no bytes read.
    Eof,
    /// A line (including its `\n` when present) is in `buf`.
    Line,
    /// The line exceeded `max_bytes`; the rest was discarded up to `\n`.
    TooLong,
}

/// Reads one `\n`-terminated line into `buf`.
///
/// `buf` is cleared first, and the reader never holds more than `max_bytes` bytes (the newline counts). On overflow the
/// rest of the line is discarded so the connection stays in sync, `buf` stays
/// empty and `TooLong` is returned. UTF-8 is validated only at the end; the
/// whole line is consumed even when it is invalid.
pub fn read_line_limited<R: BufRead>(
    reader: &mut R,
    buf: &mut String,
    max_bytes: usize,
) -> io::Result<LineRead> {
    buf.clear();
    let mut bytes: Vec<u8> = Vec::new();
    let mut too_long = false;
    loop {
        let available = match reader.fill_buf() {
            Ok(a) => a,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        if available.is_empty() {
            break;
        }
        let (chunk_len, found_newline) = available
            .iter()
            .position(|&b| b == b'\n')
            .map_or((available.len(), false), |i| (i + 1, true));
        if !too_long {
            if bytes.len() + chunk_len > max_bytes {
                too_long = true;
                bytes = Vec::new();
            } else {
                bytes.extend_from_slice(&available[..chunk_len]);
            }
        }
        reader.consume(chunk_len);
        if found_newline {
            break;
        }
    }
    if too_long {
        return Ok(LineRead::TooLong);
    }
    if bytes.is_empty() {
        return Ok(LineRead::Eof);
    }
    String::from_utf8(bytes).map_or_else(
        |_| {
            Err(io::Error::new(
                io::ErrorKind::InvalidData,
                INVALID_UTF8_MESSAGE,
            ))
        },
        |s| {
            *buf = s;
            Ok(LineRead::Line)
        },
    )
}
