//! Bounded line reader for the IPC server (task I1).
//!
//! A peer must not be able to make the service buffer an unbounded amount of
//! memory by never sending a newline.
use std::io::{self, BufRead};

/// Fixed message for invalid UTF-8, so it never echoes request bytes.
pub const INVALID_UTF8_MESSAGE: &str = "request line is not valid UTF-8";

/// Inner error of the `InvalidData` `io::Error` returned for a line that is not
/// UTF-8. It lets callers tell it apart from an `InvalidData` raised by the
/// transport itself (see [`is_invalid_utf8`]).
#[derive(Debug)]
pub struct InvalidUtf8Line;

impl std::fmt::Display for InvalidUtf8Line {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(INVALID_UTF8_MESSAGE)
    }
}

impl std::error::Error for InvalidUtf8Line {}

/// True only for the error produced by `read_line_limited` for non-UTF-8 input.
#[must_use]
pub fn is_invalid_utf8(err: &io::Error) -> bool {
    err.get_ref()
        .is_some_and(<dyn std::error::Error + Send + Sync>::is::<InvalidUtf8Line>)
}

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
/// `buf` is cleared first, and the reader never holds more than `max_bytes`
/// bytes (the newline counts). On overflow the rest of the line is discarded so
/// the connection stays in sync, `buf` stays empty and `TooLong` is returned.
/// UTF-8 is validated only at the end; the whole line is consumed even when it
/// is invalid (error: `InvalidData` carrying [`InvalidUtf8Line`]).
pub fn read_line_limited<R: BufRead>(
    reader: &mut R,
    buf: &mut String,
    max_bytes: usize,
) -> io::Result<LineRead> {
    buf.clear();
    let Some(bytes) = read_bounded(reader, max_bytes)? else {
        return Ok(LineRead::TooLong);
    };
    if bytes.is_empty() {
        return Ok(LineRead::Eof);
    }
    String::from_utf8(bytes).map_or_else(
        |_| Err(io::Error::new(io::ErrorKind::InvalidData, InvalidUtf8Line)),
        |s| {
            *buf = s;
            Ok(LineRead::Line)
        },
    )
}

/// Collects one line; `None` means it exceeded `max_bytes` (and was discarded).
/// The buffer capacity never exceeds `max_bytes` (`reserve_exact`, no doubling).
fn read_bounded<R: BufRead>(reader: &mut R, max_bytes: usize) -> io::Result<Option<Vec<u8>>> {
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
                bytes.reserve_exact(chunk_len);
                bytes.extend_from_slice(&available[..chunk_len]);
            }
        }
        reader.consume(chunk_len);
        if found_newline {
            break;
        }
    }
    Ok(if too_long { None } else { Some(bytes) })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    /// Yields a 100-byte chunk first, then 8 KiB chunks, without newline.
    struct Chunky {
        sent: usize,
        total: usize,
        block: Vec<u8>,
    }

    impl io::Read for Chunky {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Ok(0)
        }
    }

    impl BufRead for Chunky {
        fn fill_buf(&mut self) -> io::Result<&[u8]> {
            if self.sent >= self.total {
                return Ok(&[]);
            }
            let want = if self.sent == 0 { 100 } else { 8192 };
            let n = want.min(self.total - self.sent);
            self.block = vec![b'a'; n];
            Ok(&self.block)
        }
        fn consume(&mut self, amt: usize) {
            self.sent += amt;
        }
    }

    #[test]
    fn capacity_never_exceeds_max_bytes() {
        let max = 20_000;
        let mut r = Chunky {
            sent: 0,
            total: max,
            block: Vec::new(),
        };
        let bytes = read_bounded(&mut r, max).unwrap().unwrap();
        assert_eq!(bytes.len(), max);
        assert!(bytes.capacity() <= max, "capacity {}", bytes.capacity());
    }
}
