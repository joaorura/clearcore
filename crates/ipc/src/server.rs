#![forbid(unsafe_code)]

use crate::protocol::{IpcCommand, IpcRequest, IpcResponse, IpcStatus, handle_request};
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
        match IpcRequest::from_json(line) {
            Ok(req) => {
                let resp = handle_request(&req, handler);
                resp.to_json().unwrap_or_else(|_| "{}".to_string())
            }
            Err(e) => {
                let resp = IpcResponse::error(
                    "unknown",
                    IpcStatus::InvalidCommand,
                    "JSON_PARSE_ERROR",
                    e.to_string(),
                );
                resp.to_json().unwrap_or_else(|_| "{}".to_string())
            }
        }
    }

    pub fn handle_stream<R, W, F>(
        &self,
        mut reader: R,
        mut writer: W,
        mut handler: F,
    ) -> io::Result<()>
    where
        R: BufRead,
        W: Write,
        F: FnMut(&IpcCommand, &Value) -> IpcResponse,
    {
        let mut line = String::new();
        while reader.read_line(&mut line)? > 0 {
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                let response_json = self.handle_line(trimmed, &mut handler);
                writer.write_all(response_json.as_bytes())?;
                writer.write_all(b"\n")?;
                writer.flush()?;
            }
            line.clear();
        }
        Ok(())
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
