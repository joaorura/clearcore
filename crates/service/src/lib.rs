#![forbid(unsafe_code)]
#![allow(
    clippy::missing_errors_doc,
    clippy::missing_const_for_fn,
    clippy::too_many_lines
)]

pub mod bootstrap;
pub mod install;
pub mod settings;
pub mod voice_intake;
pub mod voice_samples;

pub use voice_intake::{IntakeTake, VoiceIntakeEngine, VoiceIntakeError};
pub use voice_samples::{
    PROFILE_BIN_BYTES, PROFILE_BIN_FILE_NAME, SAMPLES_FILE_NAME, VOICE_EMBEDDING_DIM, VoiceSample,
    VoiceSampleError, VoiceSampleManager,
};

use realtime_noise_ipc::{IpcCommand, IpcResponse, IpcServer, IpcStatus};
use realtime_noise_model::{ProfileStore, VoiceProfile};
use realtime_noise_supervisor::{
    BackendResolutionInfo, EngineSupervisor, convert_dsp_preset_to_ipc, convert_engine_mode_to_ipc,
    convert_ipc_mode_to_engine, convert_ipc_preset_to_dsp, find_repo_root, find_stateful_model_dir,
};
use serde_json::json;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub struct ServiceDaemon {
    supervisor: EngineSupervisor,
    server: IpcServer,
    shutdown: bool,
    served_client_count: usize,
    profile_store: Option<ProfileStore>,
    active_voice_profile_id: Option<String>,
    voice_samples: VoiceSampleManager,
    voice_intake: VoiceIntakeEngine,
    settings: settings::Settings,
    settings_path: Option<PathBuf>,
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
        let default_dir = VoiceSampleManager::default_dir();
        let voice_samples = VoiceSampleManager::load(&default_dir)
            .unwrap_or_else(|_| VoiceSampleManager::new(&default_dir));
        let voice_intake = VoiceIntakeEngine::new(Some(default_dir));
        Self {
            supervisor,
            server: IpcServer::new(),
            shutdown: false,
            served_client_count: 0,
            profile_store: None,
            active_voice_profile_id: None,
            voice_samples,
            voice_intake,
            settings: settings::Settings::default(),
            settings_path: None,
            model_dir,
            repo_root,
        }
    }

    #[must_use]
    pub fn with_supervisor(supervisor: EngineSupervisor) -> Self {
        let default_dir = VoiceSampleManager::default_dir();
        let voice_samples = VoiceSampleManager::load(&default_dir)
            .unwrap_or_else(|_| VoiceSampleManager::new(&default_dir));
        let voice_intake = VoiceIntakeEngine::new(Some(default_dir));
        let settings = settings::Settings {
            version: settings::SETTINGS_VERSION,
            preset: supervisor.dsp_preset(),
        };
        Self {
            supervisor,
            server: IpcServer::new(),
            shutdown: false,
            served_client_count: 0,
            profile_store: None,
            active_voice_profile_id: None,
            voice_samples,
            voice_intake,
            settings,
            settings_path: None,
            model_dir: None,
            repo_root: None,
        }
    }

    #[must_use]
    pub fn with_settings(settings: settings::Settings, settings_path: Option<PathBuf>) -> Self {
        let mut daemon = Self::new();
        daemon.supervisor.set_dsp_preset(settings.preset);
        daemon.settings = settings;
        daemon.settings_path = settings_path;
        daemon
    }

    #[must_use]
    pub fn studio_control(&self) -> Arc<studio_dsp::StudioControl> {
        self.supervisor.studio_control()
    }

    #[must_use]
    pub const fn settings(&self) -> settings::Settings {
        self.settings
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
        let dir = store.dir().to_path_buf();
        self.voice_samples =
            VoiceSampleManager::load(&dir).unwrap_or_else(|_| VoiceSampleManager::new(&dir));
        self.voice_intake = VoiceIntakeEngine::new(Some(dir));
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
    pub const fn voice_samples(&self) -> &VoiceSampleManager {
        &self.voice_samples
    }

    pub fn voice_samples_mut(&mut self) -> &mut VoiceSampleManager {
        &mut self.voice_samples
    }

    #[must_use]
    pub const fn voice_intake(&self) -> &VoiceIntakeEngine {
        &self.voice_intake
    }

    pub fn voice_intake_mut(&mut self) -> &mut VoiceIntakeEngine {
        &mut self.voice_intake
    }

    #[must_use]
    pub fn with_voice_managers(
        mut self,
        samples: VoiceSampleManager,
        intake: VoiceIntakeEngine,
    ) -> Self {
        self.voice_samples = samples;
        self.voice_intake = intake;
        self
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
        self.supervisor.select_backend(
            request,
            self.model_dir.as_deref(),
            self.repo_root.as_deref(),
        )
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
                        let preset_ipc = convert_dsp_preset_to_ipc(status.dsp_preset);
                        let desc = self.supervisor.active_backend_descriptor();
                        IpcResponse::success(
                            "status-resp",
                            json!({
                                "state": format!("{:?}", status.state),
                                "is_terminal": self.supervisor.is_terminal(),
                                "can_restart": self.supervisor.can_restart(),
                                "mode": mode_ipc,
                                "preset": preset_ipc,
                                "dsp_preset": preset_ipc,
                                "crash_count_15m": status.crash_count_15m,
                                "total_crashes": status.total_crashes,
                                "active_voice_profile_id": self.active_voice_profile_id,
                                // A profile is stored and selected, but the engine does not apply it to
                                // the audio yet (no `set_voice_profile` on `InferenceBackend`), so
                                // `is_voice_profile_active` stays false until that wiring exists.
                                "voice_profile_selected": self.active_voice_profile_id.is_some(),
                                "is_voice_profile_active": false,
                                "voice_samples_count": self.voice_samples.list_samples().len(),
                                "has_voice_profile": self.voice_samples.compute_profile_embedding().is_some(),
                                "intake_pending_count": self.voice_intake.list_pending().len(),
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
                    IpcCommand::SetPreset(ipc_preset) => {
                        let dsp_preset = convert_ipc_preset_to_dsp(*ipc_preset);
                        self.supervisor.set_dsp_preset(dsp_preset);
                        self.settings.preset = dsp_preset;
                        let persisted = settings::persist_settings(self.settings, self.settings_path.as_deref());
                        IpcResponse::success(
                            "set-preset-resp",
                            json!({
                                "preset": ipc_preset,
                                "dsp_preset": ipc_preset,
                                "success": true,
                                "persisted": persisted,
                            }),
                        )
                    }
                    IpcCommand::GetPreset => {
                        let preset_ipc = convert_dsp_preset_to_ipc(self.supervisor.dsp_preset());
                        IpcResponse::success(
                            "get-preset-resp",
                            json!({
                                "preset": preset_ipc,
                                "dsp_preset": preset_ipc,
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
                        let preset_ipc = convert_dsp_preset_to_ipc(self.supervisor.dsp_preset());
                        IpcResponse::success(
                            "diagnostics-resp",
                            json!({
                                "diagnostics": diag,
                                "total_crashes": self.supervisor.status().total_crashes,
                                "preset": preset_ipc,
                                "dsp_preset": preset_ipc,
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
                    IpcCommand::ListVoiceSamples => {
                        let samples = self.voice_samples.list_samples();
                        let samples_json: Vec<_> = samples
                            .iter()
                            .map(|s| {
                                json!({
                                    "id": s.id,
                                    "timestamp": s.timestamp,
                                    "name": s.name,
                                    "audio_path": s.audio_path,
                                    "is_active": s.is_active,
                                })
                            })
                            .collect();
                        IpcResponse::success(
                            "list-voice-samples-resp",
                            json!({
                                "samples": samples_json,
                                "total_count": samples.len(),
                                "has_profile": self.voice_samples.compute_profile_embedding().is_some(),
                            }),
                        )
                    }
                    IpcCommand::AddVoiceSample { sample_json } => {
                        match serde_json::from_str::<voice_samples::VoiceSample>(sample_json) {
                            Ok(sample) => {
                                let sample_id = sample.id.clone();
                                match self.voice_samples.add_sample(sample) {
                                    Ok(()) => {
                                        let has_profile = self.voice_samples.compute_profile_embedding().is_some();
                                        IpcResponse::success(
                                            "add-voice-sample-resp",
                                            json!({
                                                "success": true,
                                                "sample_id": sample_id,
                                                "has_profile": has_profile,
                                            }),
                                        )
                                    }
                                    Err(_) => IpcResponse::internal_error(
                                        "add-voice-sample-resp",
                                        "Failed to persist voice sample",
                                    ),
                                }
                            }
                            Err(_) => IpcResponse::invalid_command(
                                "add-voice-sample-resp",
                                "Invalid voice sample payload",
                            ),
                        }
                    }
                    IpcCommand::DeleteVoiceSample { id } => {
                        match self.voice_samples.delete_sample(id, true) {
                            Ok(deleted) => {
                                let has_profile = self.voice_samples.compute_profile_embedding().is_some();
                                IpcResponse::success(
                                    "delete-voice-sample-resp",
                                    json!({
                                        "success": true,
                                        "deleted": deleted,
                                        "has_profile": has_profile,
                                    }),
                                )
                            }
                            Err(_) => IpcResponse::internal_error(
                                "delete-voice-sample-resp",
                                "Failed to delete voice sample",
                            ),
                        }
                    }
                    IpcCommand::GetVoiceProfileEmbedding => {
                        self.voice_samples.compute_profile_embedding().map_or_else(
                            || {
                                IpcResponse::success(
                                    "get-voice-profile-embedding-resp",
                                    json!({
                                        "has_profile": false,
                                        "embedding": null,
                                    }),
                                )
                            },
                            |embedding| {
                                IpcResponse::success(
                                    "get-voice-profile-embedding-resp",
                                    json!({
                                        "has_profile": true,
                                        "dimension": voice_samples::VOICE_EMBEDDING_DIM,
                                        "embedding": embedding.as_slice(),
                                    }),
                                )
                            },
                        )
                    }
                    IpcCommand::ListIntakeSuggestions => {
                        let pending = self.voice_intake.list_pending();
                        let suggestions_json: Vec<_> = pending
                            .iter()
                            .map(|t| {
                                json!({
                                    "id": t.id,
                                    "timestamp": t.timestamp,
                                    "duration_secs": t.duration_secs,
                                    "snr": t.snr,
                                    "audio_path": t.audio_path,
                                })
                            })
                            .collect();
                        IpcResponse::success(
                            "list-intake-suggestions-resp",
                            json!({
                                "suggestions": suggestions_json,
                                "count": pending.len(),
                            }),
                        )
                    }
                    IpcCommand::AddIntakeSuggestion { take_json } => {
                        match serde_json::from_str::<voice_intake::IntakeTake>(take_json) {
                            Ok(take) => {
                                let take_id = take.id.clone();
                                match self.voice_intake.add_take(take) {
                                    Ok(()) => IpcResponse::success(
                                        "add-intake-suggestion-resp",
                                        json!({
                                            "success": true,
                                            "take_id": take_id,
                                        }),
                                    ),
                                    Err(_) => IpcResponse::internal_error(
                                        "add-intake-suggestion-resp",
                                        "Failed to add intake suggestion",
                                    ),
                                }
                            }
                            Err(_) => IpcResponse::invalid_command(
                                "add-intake-suggestion-resp",
                                "Invalid intake suggestion payload",
                            ),
                        }
                    }
                    IpcCommand::ApproveIntakeSuggestion { id, name } => {
                        match self.voice_intake.approve_take(id, &mut self.voice_samples, name.as_deref()) {
                            Ok(sample) => {
                                let has_profile = self.voice_samples.compute_profile_embedding().is_some();
                                IpcResponse::success(
                                    "approve-intake-suggestion-resp",
                                    json!({
                                        "success": true,
                                        "approved": true,
                                        "sample_id": sample.id,
                                        "has_profile": has_profile,
                                    }),
                                )
                            }
                            Err(voice_intake::VoiceIntakeError::TakeNotFound(_)) => {
                                IpcResponse::error(
                                    "approve-intake-suggestion-resp",
                                    IpcStatus::InvalidCommand,
                                    "TAKE_NOT_FOUND",
                                    "Intake suggestion not found",
                                )
                            }
                            Err(_) => IpcResponse::internal_error(
                                "approve-intake-suggestion-resp",
                                "Failed to approve intake suggestion",
                            ),
                        }
                    }
                    IpcCommand::DiscardIntakeSuggestion { id } => {
                        match self.voice_intake.discard_take(id) {
                            Ok(true) => IpcResponse::success(
                                "discard-intake-suggestion-resp",
                                json!({
                                    "success": true,
                                    "discarded": true,
                                }),
                            ),
                            Ok(false) => IpcResponse::success(
                                "discard-intake-suggestion-resp",
                                json!({
                                    "success": true,
                                    "discarded": false,
                                }),
                            ),
                            Err(_) => IpcResponse::internal_error(
                                "discard-intake-suggestion-resp",
                                "Failed to discard intake suggestion",
                            ),
                        }
                    }
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
