#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const PROTOCOL_VERSION: &str = "realtime-noise.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum DenoiseMode {
    Active,
    Bypass,
    Mute,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum IpcStatus {
    Ok,
    VersionMismatch,
    Unauthorized,
    InvalidCommand,
    InternalError,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BackendPayload {
    Named { backend: String },
    Direct(String),
}

impl BackendPayload {
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Named { backend } => backend.as_str(),
            Self::Direct(s) => s.as_str(),
        }
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum IpcCommand {
    GetStatus,
    SetMode(DenoiseMode),
    RestartGeneration,
    GetDiagnostics,
    Shutdown,
    /// Activates a voice profile. `profile_json` carries the serialized `VoiceProfile`
    /// (biometric data: it must never be logged or echoed back in responses).
    SetVoiceProfile {
        profile_json: String,
    },
    ClearVoiceProfile,
    SetBackend(BackendPayload),
    GetBackend,
}

impl IpcCommand {
    #[must_use]
    pub fn set_backend(backend: impl Into<String>) -> Self {
        Self::SetBackend(BackendPayload::Direct(backend.into()))
    }
}

/// Manual `Debug`: `SetVoiceProfile` carries biometric data, so its payload is redacted and can
/// never reach a log through `{:?}` (also reached via `IpcRequest`'s derived `Debug`).
impl std::fmt::Debug for IpcCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::GetStatus => f.write_str("GetStatus"),
            Self::SetMode(mode) => f.debug_tuple("SetMode").field(mode).finish(),
            Self::RestartGeneration => f.write_str("RestartGeneration"),
            Self::GetDiagnostics => f.write_str("GetDiagnostics"),
            Self::Shutdown => f.write_str("Shutdown"),
            Self::SetVoiceProfile { .. } => f
                .debug_struct("SetVoiceProfile")
                .field("profile_json", &format_args!("<redacted>"))
                .finish(),
            Self::ClearVoiceProfile => f.write_str("ClearVoiceProfile"),
            Self::SetBackend(payload) => f
                .debug_tuple("SetBackend")
                .field(&payload.as_str())
                .finish(),
            Self::GetBackend => f.write_str("GetBackend"),
        }
    }
}


#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IpcErrorDetail {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IpcRequest {
    pub version: String,
    pub request_id: String,
    pub command: IpcCommand,
    pub payload: Value,
}

impl IpcRequest {
    #[must_use]
    pub fn new(command: IpcCommand, payload: Value) -> Self {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        Self {
            version: PROTOCOL_VERSION.to_string(),
            request_id: format!("req-{ts}"),
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IpcResponse {
    pub version: String,
    pub request_id: String,
    pub status: IpcStatus,
    pub payload: Value,
    pub error: Option<IpcErrorDetail>,
}

impl IpcResponse {
    #[must_use]
    pub fn success(request_id: impl Into<String>, payload: Value) -> Self {
        Self {
            version: PROTOCOL_VERSION.to_string(),
            request_id: request_id.into(),
            status: IpcStatus::Ok,
            payload,
            error: None,
        }
    }

    #[must_use]
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

    #[must_use]
    pub fn version_mismatch(request_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self::error(
            request_id,
            IpcStatus::VersionMismatch,
            "VERSION_MISMATCH",
            message,
        )
    }

    #[must_use]
    pub fn invalid_command(request_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self::error(
            request_id,
            IpcStatus::InvalidCommand,
            "INVALID_COMMAND",
            message,
        )
    }

    #[must_use]
    pub fn internal_error(request_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self::error(
            request_id,
            IpcStatus::InternalError,
            "INTERNAL_ERROR",
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

/// Dispatches an IPC request to a handler, enforcing strict protocol version checks.
pub fn handle_request<F>(req: &IpcRequest, mut handler: F) -> IpcResponse
where
    F: FnMut(&IpcCommand, &Value) -> IpcResponse,
{
    if req.version != PROTOCOL_VERSION {
        return IpcResponse::version_mismatch(
            &req.request_id,
            format!(
                "Incompatible protocol version '{}', expected '{}'",
                req.version, PROTOCOL_VERSION
            ),
        );
    }
    handler(&req.command, &req.payload)
}
