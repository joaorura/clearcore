#![forbid(unsafe_code)]

pub mod bootstrap;
pub mod install;

use std::io::{self, BufRead, Write};
use realtime_noise_ipc::{IpcCommand, IpcResponse, IpcServer};
use realtime_noise_supervisor::EngineSupervisor;
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

    #[must_use]
    pub const fn is_shutdown(&self) -> bool {
        self.shutdown
    }

    pub fn serve_client<R: BufRead, W: Write>(&mut self, mut reader: R, mut writer: W) -> io::Result<()> {
        // RED: Intentionally refuse subsequent client connection to verify test failure
        if self.served_client_count > 0 {
            return Err(io::Error::new(
                io::ErrorKind::ConnectionRefused,
                "RED: daemon refused second client connection",
            ));
        }
        self.served_client_count += 1;

        let mut line = String::new();
        while reader.read_line(&mut line)? > 0 {
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                let resp_str = self.server.handle_line(trimmed, |cmd, _payload| {
                    match cmd {
                        IpcCommand::GetStatus => {
                            let status = self.supervisor.status();
                            IpcResponse::success(
                                "status-resp",
                                json!({
                                    "state": format!("{:?}", status.state),
                                    "crash_count_15m": status.crash_count_15m,
                                    "total_crashes": status.total_crashes,
                                }),
                            )
                        }
                        IpcCommand::Shutdown => {
                            self.shutdown = true;
                            IpcResponse::success("shutdown-resp", json!({"shutdown": true}))
                        }
                        _ => IpcResponse::success("ok", json!({})),
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
