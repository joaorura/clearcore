#![forbid(unsafe_code)]

use std::io::{self, BufRead, Write};
use crate::protocol::{handle_request, IpcCommand, IpcRequest, IpcResponse, IpcStatus};
use serde_json::Value;

pub struct IpcServer;

impl Default for IpcServer {
    fn default() -> Self {
        Self::new()
    }
}

impl IpcServer {
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

    pub fn handle_stream<R, W, F>(&self, mut reader: R, mut writer: W, mut handler: F) -> io::Result<()>
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
