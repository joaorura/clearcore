#![forbid(unsafe_code)]
#![allow(
    clippy::missing_errors_doc,
    clippy::missing_const_for_fn,
    clippy::too_many_lines
)]

pub mod bootstrap;
pub mod install;

use realtime_noise_ipc::{IpcCommand, IpcResponse, IpcServer, IpcStatus};
use realtime_noise_model::{ProfileStore, VoiceProfile};
use realtime_noise_supervisor::{
    BackendResolutionInfo, EngineSupervisor, convert_engine_mode_to_ipc,
    convert_ipc_mode_to_engine, find_repo_root, find_stateful_model_dir,
};
use serde_json::json;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

pub struct ServiceDaemon {
    supervisor: EngineSupervisor,
    server: IpcServer,
    shutdown: bool,
    served_client_count: usize,
    profile_store: Option<ProfileStore>,
    active_voice_profile_id: Option<String>,
    model_dir: Option<PathBuf>,
    repo_root: Option<PathBuf>,
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
        let model_dir = find_stateful_model_dir();
        let repo_root = find_repo_root();
        let mut supervisor = EngineSupervisor::default();
        let _ = supervisor.select_backend("auto", model_dir.as_deref(), repo_root.as_deref());
        Self {
            supervisor,
            server: IpcServer::new(),
            shutdown: false,
            served_client_count: 0,
            profile_store: None,
            active_voice_profile_id: None,
            model_dir,
            repo_root,
        }
    }

    #[must_use]
    pub fn with_supervisor(supervisor: EngineSupervisor) -> Self {
        Self {
            supervisor,
            server: IpcServer::new(),
            shutdown: false,
            served_client_count: 0,
            profile_store: None,
            active_voice_profile_id: None,
            model_dir: None,
            repo_root: None,
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

    pub fn select_backend(&mut self, request: &str) -> BackendResolutionInfo {
        self.supervisor.select_backend(request, self.model_dir.as_deref(), self.repo_root.as_deref())
    }

    pub fn set_model_dir(&mut self, path: PathBuf) {
        self.model_dir = Some(path);
    }

    pub fn set_repo_root(&mut self, path: PathBuf) {
        self.repo_root = Some(path);
    }

    #[must_use]
    pub fn model_dir(&self) -> Option<&Path> {
        self.model_dir.as_deref()
    }

    #[must_use]
    pub fn repo_root(&self) -> Option<&Path> {
        self.repo_root.as_deref()
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
                        let desc = self.supervisor.active_backend_descriptor();
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
                                "active_backend": self.supervisor.active_backend_name(),
                                "requested_backend": self.supervisor.requested_backend_name(),
                                "is_hardware_accelerated": self.supervisor.is_hardware_accelerated(),
                                "backend_runtime": desc.as_ref().map(|d| d.runtime),
                                "backend_device": self.supervisor.active_backend_device(),
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
                    IpcCommand::SetBackend(payload) => {
                        let req_name = payload.as_str();
                        let info = self.supervisor.select_backend(
                            req_name,
                            self.model_dir.as_deref(),
                            self.repo_root.as_deref(),
                        );
                        IpcResponse::success(
                            "set-backend-resp",
                            json!({
                                "success": true,
                                "requested_backend": req_name,
                                "active_backend": info.name,
                                "is_hardware_accelerated": info.is_hardware_accelerated,
                                "device": info.device,
                                "runtime": info.runtime,
                                "fallback": info.is_fallback,
                                "fallback_reason": info.fallback_reason,
                            }),
                        )
                    }
                    IpcCommand::GetBackend => {
                        let desc = self.supervisor.active_backend_descriptor();
                        IpcResponse::success(
                            "get-backend-resp",
                            json!({
                                "active_backend": self.supervisor.active_backend_name(),
                                "requested_backend": self.supervisor.requested_backend_name(),
                                "is_hardware_accelerated": self.supervisor.is_hardware_accelerated(),
                                "backend_runtime": desc.as_ref().map(|d| d.runtime),
                                "backend_device": self.supervisor.active_backend_device(),
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
