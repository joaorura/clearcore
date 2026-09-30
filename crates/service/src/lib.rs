#![forbid(unsafe_code)]
#![allow(clippy::missing_errors_doc, clippy::missing_const_for_fn)]

pub mod bootstrap;
pub mod install;

use std::io::{self, BufRead, Write};
use realtime_noise_ipc::{
    IpcCommand, IpcResponse, IpcServer, IpcStatus,
};
use realtime_noise_supervisor::{
    convert_engine_mode_to_ipc, convert_ipc_mode_to_engine, EngineSupervisor,
};
use serde_json::json;

pub struct ServiceDaemon {
    supervisor: EngineSupervisor,
    server: IpcServer,
    shutdown: bool,
    served_client_count: usize,
}

impl Default for ServiceDaemon {
    fn default() -> Self {
        Self::new()
    }
}

impl ServiceDaemon {
    #[must_use]
    pub fn new() -> Self {
        Self {
            supervisor: EngineSupervisor::default(),
            server: IpcServer::new(),
            shutdown: false,
            served_client_count: 0,
        }
    }

    #[must_use]
    pub const fn supervisor(&self) -> &EngineSupervisor {
        &self.supervisor
    }

    pub fn supervisor_mut(&mut self) -> &mut EngineSupervisor {
        &mut self.supervisor
    }

    #[must_use]
    pub const fn is_shutdown(&self) -> bool {
        self.shutdown
    }

    #[must_use]
    pub const fn served_client_count(&self) -> usize {
        self.served_client_count
    }

    /// Serves a control client connection stream until the client disconnects (EOF).
    /// Safe client disconnect: Daemon outlives UI disconnects and accepts subsequent client connections.
    pub fn serve_client<R: BufRead, W: Write>(&mut self, mut reader: R, mut writer: W) -> io::Result<()> {
        self.served_client_count += 1;

        let mut line = String::new();
        while reader.read_line(&mut line)? > 0 {
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                let resp_str = self.server.handle_line(trimmed, |cmd, _payload| {
                    match cmd {
                        IpcCommand::GetStatus => {
                            let status = self.supervisor.status();
                            let mode_ipc = convert_engine_mode_to_ipc(status.active_mode);
                            IpcResponse::success(
                                "status-resp",
                                json!({
                                    "state": format!("{:?}", status.state),
                                    "is_terminal": self.supervisor.is_terminal(),
                                    "can_restart": self.supervisor.can_restart(),
                                    "mode": mode_ipc,
                                    "crash_count_15m": status.crash_count_15m,
                                    "total_crashes": status.total_crashes,
                                }),
                            )
                        }
                        IpcCommand::SetMode(ipc_mode) => {
                            let engine_mode = convert_ipc_mode_to_engine(*ipc_mode);
                            self.supervisor.set_mode(engine_mode);
                            IpcResponse::success(
                                "set-mode-resp",
                                json!({
                                    "mode": ipc_mode,
                                    "success": true,
                                }),
                            )
                        }
                        IpcCommand::RestartGeneration => {
                            match self.supervisor.reset() {
                                Ok(()) => IpcResponse::success(
                                    "restart-resp",
                                    json!({"restarted": true, "state": "Running"}),
                                ),
                                Err(err) => IpcResponse::error(
                                    "restart-resp",
                                    IpcStatus::InternalError,
                                    "RESTART_FAILED",
                                    err.to_string(),
                                ),
                            }
                        }
                        IpcCommand::GetDiagnostics => {
                            let diag = self.supervisor.diagnostics();
                            IpcResponse::success(
                                "diagnostics-resp",
                                json!({
                                    "diagnostics": diag,
                                    "total_crashes": self.supervisor.status().total_crashes,
                                }),
                            )
                        }
                        IpcCommand::Shutdown => {
                            self.shutdown = true;
                            IpcResponse::success("shutdown-resp", json!({"shutdown": true}))
                        }
                    }
                });
                writer.write_all(resp_str.as_bytes())?;
                writer.write_all(b"\n")?;
                writer.flush()?;
            }
            line.clear();
        }
        Ok(())
    }
}
