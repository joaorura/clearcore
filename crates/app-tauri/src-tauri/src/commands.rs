#![forbid(unsafe_code)]
#![allow(clippy::missing_errors_doc, clippy::missing_const_for_fn)]

use realtime_noise_ipc::{
    DenoiseMode, IpcClient, IpcCommand, IpcRequest, IpcResponse, IpcStatus, default_endpoint_path,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::fmt;
use std::io::{self, BufReader, BufWriter};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandError {
    pub code: String,
    pub message: String,
}

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for CommandError {}

impl From<io::Error> for CommandError {
    fn from(err: io::Error) -> Self {
        Self {
            code: "SERVICE_UNREACHABLE".to_string(),
            message: format!("Could not connect to realtime-noise-service daemon: {err}"),
        }
    }
}

pub fn execute_ipc_command(
    command: IpcCommand,
    payload: Value,
) -> Result<IpcResponse, CommandError> {
    let endpoint = default_endpoint_path();
    let request = IpcRequest::new(command, payload);

    #[cfg(unix)]
    {
        use std::os::unix::net::UnixStream;
        let stream = UnixStream::connect(&endpoint)?;
        let mut reader = BufReader::new(stream.try_clone()?);
        let mut writer = BufWriter::new(stream);
        let response = IpcClient::send_request(&mut reader, &mut writer, &request)?;
        if response.status != IpcStatus::Ok {
            let (code, msg) = response.error.map_or_else(
                || ("IPC_ERROR".to_string(), "Unknown IPC error".to_string()),
                |err| (err.code, err.message),
            );
            return Err(CommandError {
                code,
                message: msg,
            });
        }
        Ok(response)
    }

    #[cfg(not(unix))]
    {
        let _ = (&endpoint, &request);
        Err(CommandError {
            code: "SERVICE_UNAVAILABLE".to_string(),
            message: "Direct IPC stream transport not supported on this platform".to_string(),
        })
    }
}

pub fn get_status() -> Result<Value, CommandError> {
    let resp = execute_ipc_command(IpcCommand::GetStatus, json!({}))?;
    Ok(resp.payload)
}

pub fn set_mode(mode: DenoiseMode) -> Result<Value, CommandError> {
    let resp = execute_ipc_command(IpcCommand::SetMode(mode), json!({}))?;
    Ok(resp.payload)
}

pub fn restart_generation() -> Result<Value, CommandError> {
    let resp = execute_ipc_command(IpcCommand::RestartGeneration, json!({}))?;
    Ok(resp.payload)
}

pub fn get_diagnostics() -> Result<Value, CommandError> {
    let resp = execute_ipc_command(IpcCommand::GetDiagnostics, json!({}))?;
    Ok(resp.payload)
}
