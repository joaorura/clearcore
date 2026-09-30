#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const PROTOCOL_VERSION: &str = "realtime-noise.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DenoiseMode {
    Active,
    Bypass,
    Mute,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IpcStatus {
    Ok,
    VersionMismatch,
    Unauthorized,
    InvalidCommand,
    InternalError,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum IpcCommand {
    GetStatus,
    SetMode(DenoiseMode),
    RestartGeneration,
    GetDiagnostics,
    Shutdown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IpcErrorDetail {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IpcRequest {
    pub version: String,
    pub request_id: String,
    pub command: IpcCommand,
    pub payload: Value,
}

impl IpcRequest {
    pub fn new(command: IpcCommand, payload: Value) -> Self {
        Self {
            version: PROTOCOL_VERSION.to_string(),
            request_id: "req-init".to_string(),
            command,
            payload,
        }
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    pub fn from_json(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(s)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IpcResponse {
    pub version: String,
    pub request_id: String,
    pub status: IpcStatus,
    pub payload: Value,
    pub error: Option<IpcErrorDetail>,
}

impl IpcResponse {
    pub fn success(request_id: impl Into<String>, payload: Value) -> Self {
        Self {
            version: PROTOCOL_VERSION.to_string(),
            request_id: request_id.into(),
            status: IpcStatus::Ok,
            payload,
            error: None,
        }
    }

    pub fn error(
        request_id: impl Into<String>,
        status: IpcStatus,
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            version: PROTOCOL_VERSION.to_string(),
            request_id: request_id.into(),
            status,
            payload: Value::Null,
            error: Some(IpcErrorDetail {
                code: code.into(),
                message: message.into(),
            }),
        }
    }

    pub fn version_mismatch(request_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self::error(
            request_id,
            IpcStatus::VersionMismatch,
            "VERSION_MISMATCH",
            message,
        )
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    pub fn from_json(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(s)
    }
}

/// Dispatches an IPC request to a handler.
/// NOTE: In Phase 1 RED, version validation is intentionally omitted so the test fails.
pub fn handle_request<F>(req: &IpcRequest, mut handler: F) -> IpcResponse
where
    F: FnMut(&IpcCommand, &Value) -> IpcResponse,
{
    // RED: version check is NOT performed yet
    handler(&req.command, &req.payload)
}
