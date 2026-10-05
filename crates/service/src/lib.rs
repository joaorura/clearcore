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

mod enrollment_config;
mod enrollment_error;
mod enrollment_ingest;
mod enrollment_jobs;
mod ipc_voice;
mod voice_budget;
mod voice_storage_migration;

pub use enrollment_error::EnrollError;
pub use enrollment_ingest::Denoiser;
#[doc(hidden)]
pub use ipc_voice::{DenoiserFactory, EnrollmentHooks, ModelFactory};

pub use voice_intake::{IntakeTake, VoiceIntakeEngine, VoiceIntakeError};
pub use voice_samples::{
    PROFILE_BIN_BYTES, PROFILE_BIN_FILE_NAME, SAMPLES_FILE_NAME, VOICE_EMBEDDING_DIM, VoiceSample,
    VoiceSampleError, VoiceSampleManager,
};

use realtime_noise_ipc::enrollment_codes::{ENROLL_PAYLOAD_TOO_LARGE, MAX_REQUEST_LINE_BYTES};
use realtime_noise_ipc::line_limit::{LineRead, is_invalid_utf8, read_line_limited};
use realtime_noise_ipc::protocol::truncate_request_id;
use realtime_noise_ipc::{IpcCommand, IpcRequest, IpcResponse, IpcStatus, handle_request};
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
    shutdown: bool,
    served_client_count: usize,
    profile_store: Option<ProfileStore>,
    /// Id of the profile persisted on disk (may differ from the one applied to the engine).
    stored_voice_profile_id: Option<String>,
    /// Generic reason the stored profile could not be applied at startup; never carries payload.
    voice_profile_error: Option<String>,
    voice_samples: VoiceSampleManager,
    voice_intake: VoiceIntakeEngine,
    settings: settings::Settings,
    settings_path: Option<PathBuf>,
    model_dir: Option<PathBuf>,
    repo_root: Option<PathBuf>,
    /// Enrollment jobs, seams and the legacy-migration source (see `ipc_voice`).
    enrollment: ipc_voice::EnrollmentState,
}

/// Generic messages only: voice profiles are biometric data and neither their content nor the
/// received JSON may appear in a response, not even in an error.
const INVALID_PROFILE_MESSAGE: &str = "Invalid voice profile";
const PERSIST_FAILED_MESSAGE: &str = "Failed to persist voice profile";
const MISSING_MESSAGE: &str = "The stored voice profile is no longer on disk";
const NOT_APPLIED_MESSAGE: &str = "The backend could not apply the stored voice profile";
const LOAD_FAILED_MESSAGE: &str =
    "The stored voice profile failed validation or has insecure permissions";
const RESTORE_FAILED_MESSAGE: &str =
    "The previous voice profile could not be restored on the backend";
const CLEAR_FAILED_MESSAGE: &str = "The active backend could not return to the neutral voice";
const APPLY_FAILED_MESSAGE: &str = "The active backend cannot apply this voice profile";
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
        let default_dir_for_migration = default_dir.clone();
        let voice_intake = VoiceIntakeEngine::new(Some(default_dir));
        Self {
            supervisor,
            shutdown: false,
            served_client_count: 0,
            profile_store: None,
            stored_voice_profile_id: None,
            voice_profile_error: None,
            voice_samples,
            voice_intake,
            settings: settings::Settings::default(),
            settings_path: None,
            model_dir,
            repo_root,
            enrollment: ipc_voice::EnrollmentState::from_env(Some(default_dir_for_migration)),
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
            shutdown: false,
            served_client_count: 0,
            profile_store: None,
            stored_voice_profile_id: None,
            voice_profile_error: None,
            voice_samples,
            voice_intake,
            settings,
            settings_path: None,
            model_dir: None,
            repo_root: None,
            // Embedding/test seam: no legacy migration from `$HOME`.
            enrollment: ipc_voice::EnrollmentState::from_env(None),
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

    /// Attaches `store` and loads its active profile, then tries to apply it to the engine.
    /// Fail-closed: a stored profile with insecure permissions, a broken integrity hash or an
    /// invalid payload activates nothing. A valid profile the backend cannot apply stays stored
    /// (the file is kept) and the audio stays neutral; the reason is exposed in the status.
    pub fn attach_profile_store(&mut self, store: ProfileStore) {
        self.stored_voice_profile_id = None;
        self.voice_profile_error = None;
        match store.load_active() {
            Ok(Some(profile)) => {
                self.stored_voice_profile_id = Some(profile.id.clone());
                if self.supervisor.set_voice_profile(Some(&profile)).is_err() {
                    eprintln!("Stored voice profile was not applied: the backend rejected it");
                    self.voice_profile_error = Some(NOT_APPLIED_MESSAGE.to_owned());
                }
            }
            Ok(None) => {}
            Err(_) => {
                eprintln!(
                    "Stored voice profile was not loaded: it failed validation or has insecure permissions"
                );
                self.voice_profile_error = Some(LOAD_FAILED_MESSAGE.to_owned());
            }
        }
        let dir = store.dir().to_path_buf();
        // One-shot migration of the pre-pipeline gallery (spec 6.3): legacy samples arrive
        // without audio (needs_reenroll). A failure never stops the daemon and logs no detail.
        if let Some(legacy) = self.enrollment.legacy_samples_dir.clone()
            && legacy != dir
            && voice_storage_migration::migrate_legacy_into_dir(&legacy, &dir).is_err()
        {
            eprintln!("Legacy voice samples were not migrated");
        }
        self.voice_samples =
            VoiceSampleManager::load(&dir).unwrap_or_else(|_| VoiceSampleManager::new(&dir));
        self.voice_intake = VoiceIntakeEngine::new(Some(dir));
        self.profile_store = Some(store);
    }

    /// Id of the voice profile actually applied to the engine, if any.
    #[must_use]
    pub fn active_voice_profile_id(&self) -> Option<&str> {
        self.supervisor.active_voice_profile_id()
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

    /// Installs an already-built backend (test and embedding seam) and re-applies the stored
    /// voice profile to it, exactly like a `SetBackend` request does.
    pub fn install_backend(
        &mut self,
        backend: Box<dyn realtime_noise_model::InferenceBackend>,
        name: &str,
    ) {
        self.supervisor.set_backend(backend, name);
        self.reapply_stored_profile();
    }

    /// After a backend swap the supervisor only carries over the profile it had applied. Retry the
    /// stored one (a previous backend may have rejected it) and refresh the diagnostic.
    fn reapply_stored_profile(&mut self) {
        reapply_stored_profile(
            &mut self.supervisor,
            self.profile_store.as_ref(),
            &mut self.stored_voice_profile_id,
            &mut self.voice_profile_error,
        );
    }

    /// Resolves and installs a backend by name, then retries the stored voice profile on it.
    pub fn select_backend(&mut self, request: &str) -> BackendResolutionInfo {
        select_backend_and_reapply(
            &mut self.supervisor,
            request,
            self.model_dir.as_deref(),
            self.repo_root.as_deref(),
            self.profile_store.as_ref(),
            &mut self.stored_voice_profile_id,
            &mut self.voice_profile_error,
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
    ///
    /// Lines are read with a hard cap of `MAX_REQUEST_LINE_BYTES`: an oversized line is answered
    /// with `ENROLL_PAYLOAD_TOO_LARGE` and a non-UTF-8 line with a fixed parse error, and the
    /// connection keeps serving; any other read error ends the session. Finished enrollment jobs
    /// are drained (and applied on this thread) before every request.
    pub fn serve_client<R: BufRead, W: Write>(
        &mut self,
        mut reader: R,
        mut writer: W,
    ) -> io::Result<()> {
        self.served_client_count += 1;

        let mut line = String::new();
        loop {
            let resp_str = match read_line_limited(&mut reader, &mut line, MAX_REQUEST_LINE_BYTES) {
                Ok(LineRead::Eof) => return Ok(()),
                Ok(LineRead::Line) => {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    self.drain_enrollment_jobs();
                    // Same contract as `IpcServer::handle_line` (fixed parse error, version
                    // check, truncated request id), but the parsed request stays here so its
                    // audio can be wiped after the handler.
                    let resp = IpcRequest::from_json(trimmed).map_or_else(
                        |_| {
                            fixed_error_json(
                                IpcStatus::InvalidCommand,
                                "JSON_PARSE_ERROR",
                                "malformed request",
                            )
                        },
                        |mut request| {
                            let mut resp =
                                handle_request(&request, |cmd, _payload| self.handle_command(cmd));
                            wipe_request_audio(&mut request);
                            resp.request_id = truncate_request_id(&resp.request_id);
                            resp.to_json().unwrap_or_else(|_| "{}".to_owned())
                        },
                    );
                    // The line may carry raw PCM (base64): wipe it before the next read.
                    wipe_string(&mut line);
                    resp
                }
                Ok(LineRead::TooLong) => fixed_error_json(
                    IpcStatus::InvalidCommand,
                    ENROLL_PAYLOAD_TOO_LARGE,
                    "request line too large",
                ),
                Err(e) if is_invalid_utf8(&e) => fixed_error_json(
                    IpcStatus::InvalidCommand,
                    "JSON_PARSE_ERROR",
                    "malformed request",
                ),
                Err(e) => return Err(e),
            };
            writer.write_all(resp_str.as_bytes())?;
            writer.write_all(b"\n")?;
            writer.flush()?;
        }
    }

    fn handle_command(&mut self, cmd: &IpcCommand) -> IpcResponse {
        match cmd {
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
                        // Voice profile fields describe the SERVICE's own backend: the
                        // profile is "applied" when that backend accepted it. The packaged
                        // audio path (filter-capi / helper) does not use it yet (stage 2).
                        // Reported in every mode, including Mute, Bypass and
                        // TerminalSafeState.
                        "active_voice_profile_id": self.supervisor.active_voice_profile_id(),
                        "stored_voice_profile_id": self.stored_voice_profile_id,
                        "voice_profile_selected": self.stored_voice_profile_id.is_some(),
                        "is_voice_profile_active": self.supervisor.active_voice_profile_id().is_some(),
                        "voice_profile_error": self.voice_profile_error,
                        // Whether the live backend can apply a conditioned profile at all
                        // (`false` on NPU/OpenVINO and on a base model without FiLM inputs).
                        "voice_profile_supported": self.supervisor.supports_voice_profile(),
                        // The APPLIED profile carries a calibrated microphone EQ.
                        "neural_eq_calibrated": self
                            .supervisor
                            .active_voice_profile()
                            .is_some_and(|profile| profile.eq.is_some()),
                        "voice_samples_count": self.voice_samples.list_samples().len(),
                        // A stored profile, not the obsolete averaged sample embedding.
                        "has_voice_profile": self.stored_voice_profile_id.is_some(),
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
                let info = select_backend_and_reapply(
                    &mut self.supervisor,
                    req_name,
                    self.model_dir.as_deref(),
                    self.repo_root.as_deref(),
                    self.profile_store.as_ref(),
                    &mut self.stored_voice_profile_id,
                    &mut self.voice_profile_error,
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
                let persisted =
                    settings::persist_settings(self.settings, self.settings_path.as_deref());
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
            IpcCommand::SetVoiceProfile { profile_json } => {
                // A running build must not override a profile chosen after it started.
                self.enrollment.bump_generation();
                self.set_voice_profile_json(profile_json)
            }
            IpcCommand::ClearVoiceProfile => {
                // A running build must not revive a cleared profile.
                self.enrollment.bump_generation();
                clear_voice_profile(
                    &mut self.supervisor,
                    self.profile_store.as_ref(),
                    &mut self.stored_voice_profile_id,
                    &mut self.voice_profile_error,
                )
            }
            IpcCommand::ListVoiceSamples => self.list_voice_samples(),
            IpcCommand::AddVoiceSample {
                name,
                pcm_f32_le_b64,
                sample_rate,
                device_label,
                device_id_hash,
            } => self.add_voice_sample(
                name,
                pcm_f32_le_b64,
                *sample_rate,
                device_label,
                device_id_hash,
            ),
            IpcCommand::BuildVoiceProfile { name } => self.build_voice_profile(name),
            IpcCommand::GetEnrollmentJob { job_id } => self.get_enrollment_job(job_id),
            IpcCommand::DeleteVoiceSample { id } => {
                match self.voice_samples.delete_sample(id, true) {
                    // The WAV goes with the sample (confined to `samples/`); the client rebuilds.
                    // A build that may have read the deleted audio is made stale.
                    Ok(deleted) => {
                        if deleted {
                            self.enrollment.bump_generation();
                        }
                        IpcResponse::success(
                            "delete-voice-sample-resp",
                            json!({
                                "success": true,
                                "deleted": deleted,
                                "has_profile": self.stored_voice_profile_id.is_some(),
                            }),
                        )
                    }
                    Err(_) => IpcResponse::internal_error(
                        "delete-voice-sample-resp",
                        "Failed to delete voice sample",
                    ),
                }
            }
            // Deprecated (spec 5): it exposed the obsolete averaged embedding. Kept answering a
            // fixed error until the command is removed from the protocol.
            IpcCommand::GetVoiceProfileEmbedding => IpcResponse::error(
                "get-voice-profile-embedding-resp",
                IpcStatus::InvalidCommand,
                "DEPRECATED",
                "deprecated",
            ),
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
                            "speech_seconds": t.speech_seconds,
                            "device_label": t.device_label,
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
            IpcCommand::AddIntakeSuggestion { take_json } => self.add_intake_suggestion(take_json),
            IpcCommand::ApproveIntakeSuggestion { id, name } => {
                self.approve_intake_suggestion(id, name.as_deref())
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
        }
    }
}

/// Serialized error response with a fixed code and message (never request bytes).
fn fixed_error_json(status: IpcStatus, code: &str, message: &str) -> String {
    IpcResponse::error("unknown", status, code, message)
        .to_json()
        .unwrap_or_else(|_| "{}".to_owned())
}

/// Zeroes the audio-carrying fields of a parsed request (`AddVoiceSample` base64 PCM and the
/// `AddIntakeSuggestion` take JSON) once the handler is done.
///
/// Best effort, honestly bounded: copies this code does not own cannot be wiped — buffers that
/// `read_line_limited` dropped while growing or discarding a line, and any intermediate buffers
/// serde allocates while unescaping strings. The decoded PCM itself is zeroed by the handlers.
fn wipe_request_audio(request: &mut IpcRequest) {
    match &mut request.command {
        IpcCommand::AddVoiceSample { pcm_f32_le_b64, .. } => wipe_string(pcm_f32_le_b64),
        IpcCommand::AddIntakeSuggestion { take_json } => wipe_string(take_json),
        _ => {}
    }
}

/// Overwrites the string's bytes with zeros and leaves it empty.
fn wipe_string(text: &mut String) {
    let mut bytes = std::mem::take(text).into_bytes();
    bytes.fill(0);
    // Keeps the zeroing from being optimized away as a dead store.
    std::hint::black_box(&bytes);
}

/// Why [`ServiceDaemon::apply_profile`] refused a profile; carries no payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ApplyError {
    /// No `ProfileStore` is attached, so nothing could survive a restart.
    NoStore,
    /// The active backend rejected the profile; engine and disk are unchanged.
    NotApplicable,
    /// Persisting failed; the engine was put back on the previous profile.
    PersistFailed,
}

impl ServiceDaemon {
    /// Transaction: apply to the engine, persist; if persisting fails the engine goes back to the
    /// previous profile, so the applied state never diverges from what survives a restart. Used by
    /// `SetVoiceProfile` (after `from_json`) and by the drained `BuildVoiceProfile` jobs.
    pub(crate) fn apply_profile(&mut self, profile: &VoiceProfile) -> Result<(), ApplyError> {
        let Some(store) = self.profile_store.as_ref() else {
            return Err(ApplyError::NoStore);
        };
        let previous = self.supervisor.active_voice_profile().cloned();
        if self.supervisor.set_voice_profile(Some(profile)).is_err() {
            return Err(ApplyError::NotApplicable);
        }
        if store.save_active(profile).is_err() {
            restore_previous(
                &mut self.supervisor,
                previous.as_ref(),
                &mut self.voice_profile_error,
            );
            return Err(ApplyError::PersistFailed);
        }
        self.stored_voice_profile_id = Some(profile.id.clone());
        self.voice_profile_error = None;
        Ok(())
    }

    /// JSON entry point of the transaction (`SetVoiceProfile`).
    fn set_voice_profile_json(&mut self, profile_json: &str) -> IpcResponse {
        const REQUEST_ID: &str = "set-voice-profile-resp";
        if self.profile_store.is_none() {
            return IpcResponse::error(
                REQUEST_ID,
                IpcStatus::InternalError,
                "NO_PROFILE_STORE",
                NO_PROFILE_STORE_MESSAGE,
            );
        }
        let Ok(profile) = VoiceProfile::from_json(profile_json) else {
            return IpcResponse::invalid_command(REQUEST_ID, INVALID_PROFILE_MESSAGE);
        };
        match self.apply_profile(&profile) {
            Ok(()) => IpcResponse::success(
                REQUEST_ID,
                json!({"active_voice_profile_id": self.supervisor.active_voice_profile_id()}),
            ),
            Err(ApplyError::NoStore) => IpcResponse::error(
                REQUEST_ID,
                IpcStatus::InternalError,
                "NO_PROFILE_STORE",
                NO_PROFILE_STORE_MESSAGE,
            ),
            Err(ApplyError::NotApplicable) => IpcResponse::error(
                REQUEST_ID,
                IpcStatus::InternalError,
                "VOICE_PROFILE_NOT_APPLICABLE",
                APPLY_FAILED_MESSAGE,
            ),
            Err(ApplyError::PersistFailed) => {
                IpcResponse::internal_error(REQUEST_ID, PERSIST_FAILED_MESSAGE)
            }
        }
    }
}

fn clear_voice_profile(
    supervisor: &mut EngineSupervisor,
    store: Option<&ProfileStore>,
    stored_id: &mut Option<String>,
    error: &mut Option<String>,
) -> IpcResponse {
    const REQUEST_ID: &str = "clear-voice-profile-resp";
    let previous = supervisor.active_voice_profile().cloned();
    if supervisor.set_voice_profile(None).is_err() {
        // The engine keeps the profile: leave disk and reported state untouched.
        return IpcResponse::error(
            REQUEST_ID,
            IpcStatus::InternalError,
            "VOICE_PROFILE_CLEAR_FAILED",
            CLEAR_FAILED_MESSAGE,
        );
    }
    if let Some(store) = store
        && store.clear_active().is_err()
    {
        restore_previous(supervisor, previous.as_ref(), error);
        return IpcResponse::internal_error(REQUEST_ID, PERSIST_FAILED_MESSAGE);
    }
    *stored_id = None;
    *error = None;
    IpcResponse::success(REQUEST_ID, json!({"active_voice_profile_id": null}))
}

/// Puts the engine back on `previous` after a failed persist. A failed restore leaves the engine
/// and the disk disagreeing, so it is surfaced in the status instead of being swallowed.
fn restore_previous(
    supervisor: &mut EngineSupervisor,
    previous: Option<&VoiceProfile>,
    error: &mut Option<String>,
) {
    if supervisor.set_voice_profile(previous).is_err() {
        eprintln!("Previous voice profile could not be restored on the backend");
        *error = Some(RESTORE_FAILED_MESSAGE.to_owned());
    }
}

/// Single path for every backend selection (IPC `SetBackend` and `ServiceDaemon::select_backend`).
fn select_backend_and_reapply(
    supervisor: &mut EngineSupervisor,
    request: &str,
    model_dir: Option<&Path>,
    repo_root: Option<&Path>,
    store: Option<&ProfileStore>,
    stored_slot: &mut Option<String>,
    error: &mut Option<String>,
) -> BackendResolutionInfo {
    let info = supervisor.select_backend(request, model_dir, repo_root);
    reapply_stored_profile(supervisor, store, stored_slot, error);
    info
}

fn reapply_stored_profile(
    supervisor: &mut EngineSupervisor,
    store: Option<&ProfileStore>,
    stored_slot: &mut Option<String>,
    error: &mut Option<String>,
) {
    let (Some(store), Some(id)) = (store, stored_slot.clone()) else {
        return;
    };
    if supervisor.active_voice_profile_id() == Some(id.as_str()) {
        *error = None;
        return;
    }
    match store.load_active() {
        Ok(Some(profile)) => {
            *error = supervisor
                .set_voice_profile(Some(&profile))
                .err()
                .map(|_| NOT_APPLIED_MESSAGE.to_owned());
        }
        Ok(None) => {
            // The file vanished behind our back: the id no longer names anything on disk.
            *stored_slot = None;
            *error = Some(MISSING_MESSAGE.to_owned());
        }
        Err(_) => *error = Some(LOAD_FAILED_MESSAGE.to_owned()),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn audio_fields_of_a_parsed_request_are_wiped() {
        let mut add = IpcRequest::new(
            IpcCommand::AddVoiceSample {
                name: "n".into(),
                pcm_f32_le_b64: "AAAAAAAA".into(),
                sample_rate: 48_000,
                device_label: "Mic".into(),
                device_id_hash: "h".into(),
            },
            json!({}),
        );
        wipe_request_audio(&mut add);
        let IpcCommand::AddVoiceSample { pcm_f32_le_b64, .. } = &add.command else {
            panic!("variant changed");
        };
        assert!(pcm_f32_le_b64.is_empty());

        let mut take = IpcRequest::new(
            IpcCommand::AddIntakeSuggestion {
                take_json: "{\"pcm_f32_le_b64\":\"AAAA\"}".into(),
            },
            json!({}),
        );
        wipe_request_audio(&mut take);
        let IpcCommand::AddIntakeSuggestion { take_json } = &take.command else {
            panic!("variant changed");
        };
        assert!(take_json.is_empty());
    }
}
