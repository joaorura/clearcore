#![forbid(unsafe_code)]

use crate::enrollment_codes::{ENROLL_PAYLOAD_TOO_LARGE, MAX_REQUEST_LINE_BYTES};
use crate::line_limit::{LineRead, is_invalid_utf8, read_line_limited};
use crate::protocol::{
    IpcCommand, IpcRequest, IpcResponse, IpcStatus, handle_request, truncate_request_id,
};
use serde_json::Value;
use std::io::{self, BufRead, Write};

pub const DEFAULT_SOCKET_NAME: &str = "realtime-noise.sock";
pub const DEFAULT_PIPE_NAME: &str = r"\\.\pipe\realtime-noise";

/// Platform-appropriate default endpoint location.
#[must_use]
pub fn default_endpoint_path() -> String {
    #[cfg(windows)]
    {
        DEFAULT_PIPE_NAME.to_string()
    }
    #[cfg(not(windows))]
    {
        std::env::var("XDG_RUNTIME_DIR").map_or_else(
            |_| format!("/tmp/{DEFAULT_SOCKET_NAME}"),
            |runtime_dir| format!("{runtime_dir}/{DEFAULT_SOCKET_NAME}"),
        )
    }
}

pub struct IpcServer;

impl Default for IpcServer {
    fn default() -> Self {
        Self::new()
    }
}

impl IpcServer {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    pub fn handle_line<F>(&self, line: &str, handler: F) -> String
    where
        F: FnMut(&IpcCommand, &Value) -> IpcResponse,
    {
        let mut resp = IpcRequest::from_json(line).map_or_else(
            |_| {
                IpcResponse::error(
                    "unknown",
                    IpcStatus::InvalidCommand,
                    "JSON_PARSE_ERROR",
                    "malformed request",
                )
            },
            |req| handle_request(&req, handler),
        );
        resp.request_id = truncate_request_id(&resp.request_id);
        resp.to_json().unwrap_or_else(|_| "{}".to_string())
    }

    pub fn handle_stream<R, W, F>(&self, reader: R, writer: W, handler: F) -> io::Result<()>
    where
        R: BufRead,
        W: Write,
        F: FnMut(&IpcCommand, &Value) -> IpcResponse,
    {
        self.handle_stream_with_limit(reader, writer, MAX_REQUEST_LINE_BYTES, handler)
    }

    pub fn handle_stream_with_limit<R, W, F>(
        &self,
        mut reader: R,
        mut writer: W,
        max_line_bytes: usize,
        mut handler: F,
    ) -> io::Result<()>
    where
        R: BufRead,
        W: Write,
        F: FnMut(&IpcCommand, &Value) -> IpcResponse,
    {
        let mut line = String::new();
        loop {
            let response_json = match read_line_limited(&mut reader, &mut line, max_line_bytes) {
                Ok(LineRead::Eof) => return Ok(()),
                Ok(LineRead::Line) => {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    self.handle_line(trimmed, &mut handler)
                }
                Ok(LineRead::TooLong) => IpcResponse::error(
                    "unknown",
                    IpcStatus::InvalidCommand,
                    ENROLL_PAYLOAD_TOO_LARGE,
                    "request line too large",
                )
                .to_json()
                .unwrap_or_else(|_| "{}".to_string()),
                // Invalid UTF-8: the line was fully consumed, keep serving.
                Err(e) if is_invalid_utf8(&e) => IpcResponse::error(
                    "unknown",
                    IpcStatus::InvalidCommand,
                    "JSON_PARSE_ERROR",
                    "malformed request",
                )
                .to_json()
                .unwrap_or_else(|_| "{}".to_string()),
                Err(e) => return Err(e),
            };
            writer.write_all(response_json.as_bytes())?;
            writer.write_all(b"\n")?;
            writer.flush()?;
        }
    }
}

pub struct IpcClient;

impl IpcClient {
    pub fn send_request<R: BufRead, W: Write>(
        reader: &mut R,
        writer: &mut W,
        request: &IpcRequest,
    ) -> io::Result<IpcResponse> {
        let mut req_str = request
            .to_json()
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        req_str.push('\n');
        writer.write_all(req_str.as_bytes())?;
        writer.flush()?;

        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "Connection closed by server",
            ));
        }
        IpcResponse::from_json(line.trim())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }
}
