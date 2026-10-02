#![forbid(unsafe_code)]
#![allow(clippy::missing_errors_doc, clippy::missing_const_for_fn)]

pub mod bootstrap;
pub mod install;

use realtime_noise_ipc::{IpcCommand, IpcResponse, IpcServer, IpcStatus};
use realtime_noise_model::{ProfileStore, VoiceProfile};
use realtime_noise_supervisor::{
    EngineSupervisor, convert_engine_mode_to_ipc, convert_ipc_mode_to_engine,
};
use serde_json::json;
use std::io::{self, BufRead, Write};

pub struct ServiceDaemon {
    supervisor: EngineSupervisor,
    server: IpcServer,
    shutdown: bool,
    served_client_count: usize,
    profile_store: Option<ProfileStore>,
    active_voice_profile_id: Option<String>,
}

/// Generic messages only: voice profiles are biometric data and neither their content nor the
/// received JSON may appear in a response, not even in an error.
const INVALID_PROFILE_MESSAGE: &str = "Invalid voice profile";
const PERSIST_FAILED_MESSAGE: &str = "Failed to persist voice profile";
const NO_PROFILE_STORE_MESSAGE: &str = "Voice profile storage is not configured";

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
            profile_store: None,
            active_voice_profile_id: None,
        }
    }

    /// Builds a daemon backed by `store` and activates the stored profile, if any.
    #[must_use]
    pub fn with_profile_store(store: ProfileStore) -> Self {
        let mut daemon = Self::new();
        daemon.attach_profile_store(store);
        daemon
    }

    /// Attaches `store` and loads its active profile. Fail-closed: a stored profile with insecure
    /// permissions, a broken integrity hash or an invalid payload activates nothing.
    pub fn attach_profile_store(&mut self, store: ProfileStore) {
        self.active_voice_profile_id = None;
        if let Ok(profile) = store.load_active() {
            self.active_voice_profile_id = profile.map(|profile| profile.id);
        } else {
            eprintln!(
                "Stored voice profile was not loaded: it failed validation or has insecure permissions"
            );
        }
        self.profile_store = Some(store);
    }

    #[must_use]
    pub fn active_voice_profile_id(&self) -> Option<&str> {
        self.active_voice_profile_id.as_deref()
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
    pub fn serve_client<R: BufRead, W: Write>(
        &mut self,
        mut reader: R,
        mut writer: W,
    ) -> io::Result<()> {
        self.served_client_count += 1;

        let mut line = String::new();
        while reader.read_line(&mut line)? > 0 {
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                let resp_str = self.server.handle_line(trimmed, |cmd, _payload| match cmd {
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
                                "active_voice_profile_id": self.active_voice_profile_id,
                                // A profile is stored and selected, but the engine does not apply it to
                                // the audio yet (no `set_voice_profile` on `InferenceBackend`), so
                                // `is_voice_profile_active` stays false until that wiring exists.
                                "voice_profile_selected": self.active_voice_profile_id.is_some(),
                                "is_voice_profile_active": false,
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
                    IpcCommand::RestartGeneration => match self.supervisor.reset() {
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
                    },
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
                    IpcCommand::SetVoiceProfile { profile_json } => set_voice_profile(
                        self.profile_store.as_ref(),
                        &mut self.active_voice_profile_id,
                        profile_json,
                    ),
                    IpcCommand::ClearVoiceProfile => clear_voice_profile(
                        self.profile_store.as_ref(),
                        &mut self.active_voice_profile_id,
                    ),
                    IpcCommand::Shutdown => {
                        self.shutdown = true;
                        IpcResponse::success("shutdown-resp", json!({"shutdown": true}))
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

fn set_voice_profile(
    store: Option<&ProfileStore>,
    active_id: &mut Option<String>,
    profile_json: &str,
) -> IpcResponse {
    const REQUEST_ID: &str = "set-voice-profile-resp";
    let Some(store) = store else {
        return IpcResponse::error(
            REQUEST_ID,
            IpcStatus::InternalError,
            "NO_PROFILE_STORE",
            NO_PROFILE_STORE_MESSAGE,
        );
    };
    let Ok(profile) = VoiceProfile::from_json(profile_json) else {
        return IpcResponse::invalid_command(REQUEST_ID, INVALID_PROFILE_MESSAGE);
    };
    if store.save_active(&profile).is_err() {
        return IpcResponse::internal_error(REQUEST_ID, PERSIST_FAILED_MESSAGE);
    }
    *active_id = Some(profile.id);
    IpcResponse::success(REQUEST_ID, json!({"active_voice_profile_id": active_id}))
}

fn clear_voice_profile(
    store: Option<&ProfileStore>,
    active_id: &mut Option<String>,
) -> IpcResponse {
    const REQUEST_ID: &str = "clear-voice-profile-resp";
    if let Some(store) = store
        && store.clear_active().is_err()
    {
        return IpcResponse::internal_error(REQUEST_ID, PERSIST_FAILED_MESSAGE);
    }
    *active_id = None;
    IpcResponse::success(REQUEST_ID, json!({"active_voice_profile_id": null}))
}
