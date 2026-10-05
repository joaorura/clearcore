#![forbid(unsafe_code)]

use crate::backend::{BackendResolutionInfo, instantiate_backend_with_fallback};
use crate::backoff::{BackoffTracker, MAX_CRASHES_PER_15_MINUTES};
use realtime_noise_contracts::{AudioFrame, HOP_SAMPLES};
use realtime_noise_engine::DenoiseMode;
use realtime_noise_model::{
    BackendDescriptor, InferenceBackend, InferenceError, StudioBackend, StudioResetHandle,
    VoiceProfile,
};
use std::fmt;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;
use studio_dsp::{Preset, StudioControl};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SupervisorState {
    Running,
    EngineUnavailable,
    Restarting {
        attempt: usize,
        next_retry_ms: u64,
    },
    TerminalSafeState {
        reason: String,
        diagnostic: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupervisorStatus {
    pub state: SupervisorState,
    pub crash_count_15m: usize,
    pub total_crashes: u64,
    pub active_mode: DenoiseMode,
    pub active_backend: Option<String>,
    pub dsp_preset: Preset,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SupervisorError {
    TerminalSafeState(String),
    EngineUnavailable(String),
    InvalidTransition(String),
}

impl fmt::Display for SupervisorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TerminalSafeState(msg) => write!(f, "Terminal safe state: {msg}"),
            Self::EngineUnavailable(msg) => write!(f, "Engine unavailable: {msg}"),
            Self::InvalidTransition(msg) => write!(f, "Invalid state transition: {msg}"),
        }
    }
}

impl std::error::Error for SupervisorError {}

pub struct EngineSupervisor {
    state: SupervisorState,
    mode: DenoiseMode,
    backoff: BackoffTracker,
    total_crashes: u64,
    diagnostics_log: Vec<String>,
    backend: Option<Box<dyn InferenceBackend>>,
    active_backend_name: String,
    requested_backend_name: String,
    studio_control: Arc<StudioControl>,
    studio_reset: StudioResetHandle,
    active_profile: Option<VoiceProfile>,
}

impl Default for EngineSupervisor {
    fn default() -> Self {
        Self::new(DenoiseMode::Active)
    }
}

impl EngineSupervisor {
    #[must_use]
    pub fn new(initial_mode: DenoiseMode) -> Self {
        Self {
            state: SupervisorState::Running,
            mode: initial_mode,
            backoff: BackoffTracker::new(),
            total_crashes: 0,
            diagnostics_log: Vec::new(),
            backend: None,
            active_backend_name: String::new(),
            requested_backend_name: String::new(),
            studio_control: Arc::new(StudioControl::new(Preset::Off)),
            studio_reset: StudioResetHandle::new(),
            active_profile: None,
        }
    }

    #[must_use]
    pub fn with_studio_control(mut self, control: Arc<StudioControl>) -> Self {
        self.studio_control = control;
        self
    }

    #[must_use]
    pub fn studio_control(&self) -> Arc<StudioControl> {
        Arc::clone(&self.studio_control)
    }

    #[must_use]
    pub fn dsp_preset(&self) -> Preset {
        self.studio_control.preset()
    }

    pub fn set_dsp_preset(&mut self, preset: Preset) {
        self.studio_control.set_preset(preset);
    }

    #[must_use]
    pub fn with_auto_backend(model_dir: Option<&Path>, repo_root: Option<&Path>) -> Self {
        let mut supervisor = Self::default();
        let _ = supervisor.select_backend("auto", model_dir, repo_root);
        supervisor
    }

    #[must_use]
    pub fn backend(&self) -> Option<&dyn InferenceBackend> {
        self.backend.as_deref()
    }

    pub fn backend_mut(&mut self) -> Option<&mut (dyn InferenceBackend + 'static)> {
        self.backend.as_deref_mut()
    }

    #[must_use]
    pub fn active_backend_name(&self) -> &str {
        if self.active_backend_name.is_empty() {
            "none"
        } else {
            &self.active_backend_name
        }
    }

    #[must_use]
    pub fn requested_backend_name(&self) -> &str {
        if self.requested_backend_name.is_empty() {
            "none"
        } else {
            &self.requested_backend_name
        }
    }

    #[must_use]
    pub fn is_hardware_accelerated(&self) -> bool {
        self.backend.as_ref().is_some_and(|b| {
            let desc = b.descriptor();
            desc.backend == "openvino" && desc.runtime != "openvino-cpu"
        })
    }

    #[must_use]
    pub fn active_backend_descriptor(&self) -> Option<BackendDescriptor> {
        self.backend.as_deref().map(InferenceBackend::descriptor)
    }

    #[must_use]
    pub fn active_backend_device(&self) -> Option<String> {
        self.backend.as_ref().map(|b| {
            let desc = b.descriptor();
            if desc.backend == "openvino" {
                match desc.runtime {
                    "openvino-npu" => "NPU".to_string(),
                    "openvino-gpu" => "GPU".to_string(),
                    "openvino-cpu" => "CPU".to_string(),
                    _ => "Accelerator".to_string(),
                }
            } else {
                "CPU".to_string()
            }
        })
    }

    pub fn set_backend(
        &mut self,
        backend: Box<dyn InferenceBackend>,
        active_name: impl Into<String>,
    ) {
        self.active_backend_name = active_name.into();
        let wrapped: Box<dyn InferenceBackend> = Box::new(StudioBackend::with_reset_handle(
            backend,
            Arc::clone(&self.studio_control),
            self.studio_reset.clone(),
        ));
        self.backend = Some(wrapped);
        self.reapply_voice_profile();
    }

    /// Applies `profile` to the live backend. The profile is reported as active only after the
    /// backend confirms it; on failure the previously active profile is preserved.
    pub fn set_voice_profile(
        &mut self,
        profile: Option<&VoiceProfile>,
    ) -> Result<(), InferenceError> {
        let backend = self
            .backend
            .as_mut()
            .ok_or_else(|| InferenceError::UnsupportedFeature("no active backend".into()))?;
        backend.set_voice_profile(profile)?;
        self.active_profile = profile.cloned();
        Ok(())
    }

    #[must_use]
    pub const fn active_voice_profile(&self) -> Option<&VoiceProfile> {
        self.active_profile.as_ref()
    }

    #[must_use]
    pub fn active_voice_profile_id(&self) -> Option<&str> {
        self.active_profile.as_ref().map(|p| p.id.as_str())
    }

    /// Re-applies the stored profile to a freshly installed backend. If the new backend does not
    /// confirm it, the profile is dropped so an unconfirmed profile is never reported.
    fn reapply_voice_profile(&mut self) {
        if let Some(profile) = self.active_profile.take() {
            if self.set_voice_profile(Some(&profile)).is_err() {
                self.active_profile = None;
            }
        }
    }

    pub fn select_backend(
        &mut self,
        request: &str,
        model_dir: Option<&Path>,
        repo_root: Option<&Path>,
    ) -> BackendResolutionInfo {
        self.requested_backend_name = request.to_string();
        let (backend, info) = instantiate_backend_with_fallback(request, model_dir, repo_root);
        self.set_backend(backend, &info.name);
        info
    }

    /// Process a frame according to supervisor state, denoise mode, and active backend.
    pub fn process_frame(&mut self, input: &AudioFrame) -> Result<AudioFrame, InferenceError> {
        if !self.is_running() || self.is_terminal() || self.mode == DenoiseMode::Mute {
            return Ok([0.0; HOP_SAMPLES]);
        }
        if self.mode == DenoiseMode::Bypass {
            return Ok(*input);
        }
        if let Some(backend) = &mut self.backend {
            match backend.process(input) {
                Ok(processed) => Ok(processed.samples),
                Err(err) => {
                    self.record_crash(&err.to_string(), Instant::now());
                    Err(err)
                }
            }
        } else {
            Ok(*input)
        }
    }

    #[must_use]
    pub const fn state(&self) -> &SupervisorState {
        &self.state
    }

    #[must_use]
    pub const fn mode(&self) -> DenoiseMode {
        self.mode
    }

    #[must_use]
    pub fn status(&self) -> SupervisorStatus {
        SupervisorStatus {
            state: self.state.clone(),
            crash_count_15m: self.backoff.crashes_in_window(Instant::now()),
            total_crashes: self.total_crashes,
            active_mode: self.mode,
            active_backend: if self.active_backend_name.is_empty() {
                None
            } else {
                Some(self.active_backend_name.clone())
            },
            dsp_preset: self.studio_control.preset(),
        }
    }

    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        matches!(self.state, SupervisorState::TerminalSafeState { .. })
    }

    #[must_use]
    pub const fn is_running(&self) -> bool {
        matches!(self.state, SupervisorState::Running)
    }

    #[must_use]
    pub const fn can_restart(&self) -> bool {
        !self.is_terminal()
    }

    #[must_use]
    pub fn diagnostics(&self) -> &[String] {
        &self.diagnostics_log
    }

    /// Records an engine crash, logs diagnostics, and calculates backoff or terminal safe state.
    pub fn record_crash(&mut self, reason: &str, timestamp: Instant) {
        self.total_crashes += 1;
        let log_entry = format!("Crash #{}: {}", self.total_crashes, reason);
        if self.diagnostics_log.len() >= 100 {
            self.diagnostics_log.remove(0);
        }
        self.diagnostics_log.push(log_entry);

        let limit_exceeded = self.backoff.record_crash(timestamp);
        if limit_exceeded {
            self.state = SupervisorState::TerminalSafeState {
                reason: format!(
                    "Exceeded crash limit ({MAX_CRASHES_PER_15_MINUTES} crashes per 15 minutes)"
                ),
                diagnostic: Some(reason.to_string()),
            };
        } else {
            let attempt = self.backoff.consecutive_crashes();
            let delay = self.backoff.next_backoff_delay();
            #[allow(clippy::cast_possible_truncation)]
            let next_retry_ms = delay.as_millis() as u64;
            self.state = SupervisorState::Restarting {
                attempt,
                next_retry_ms,
            };
        }
    }

    /// Attempts to restart the engine. Fails if supervisor is in `TerminalSafeState`.
    pub fn try_restart(&mut self) -> Result<(), SupervisorError> {
        if let SupervisorState::TerminalSafeState { reason, .. } = &self.state {
            return Err(SupervisorError::TerminalSafeState(format!(
                "Cannot restart while in terminal safe state ({reason}). Explicit user reset required."
            )));
        }

        self.state = SupervisorState::Running;
        Ok(())
    }

    /// User-initiated reset to clear `TerminalSafeState` and restore `Running` state.
    pub fn reset(&mut self) -> Result<(), SupervisorError> {
        self.state = SupervisorState::Running;
        self.backoff.reset();
        self.studio_reset.request();
        self.diagnostics_log
            .push("Explicit supervisor reset initiated by control client".to_string());
        Ok(())
    }

    /// Sets the active denoise operating mode.
    pub fn set_mode(&mut self, mode: DenoiseMode) {
        self.mode = mode;
    }

    /// Enforces digital silence if engine is not running or in terminal safe state.
    pub fn process_frame_or_silence(&self, frame: &mut [f32]) {
        if !self.is_running() || self.is_terminal() || self.mode == DenoiseMode::Mute {
            frame.fill(0.0);
        }
    }
}

/// Converts IPC `DenoiseMode` to engine `DenoiseMode`.
#[must_use]
pub const fn convert_ipc_mode_to_engine(mode: realtime_noise_ipc::DenoiseMode) -> DenoiseMode {
    match mode {
        realtime_noise_ipc::DenoiseMode::Active => DenoiseMode::Active,
        realtime_noise_ipc::DenoiseMode::Bypass => DenoiseMode::Bypass,
        realtime_noise_ipc::DenoiseMode::Mute => DenoiseMode::Mute,
    }
}

/// Converts engine `DenoiseMode` to IPC `DenoiseMode`.
#[must_use]
pub const fn convert_engine_mode_to_ipc(mode: DenoiseMode) -> realtime_noise_ipc::DenoiseMode {
    match mode {
        DenoiseMode::Active => realtime_noise_ipc::DenoiseMode::Active,
        DenoiseMode::Bypass => realtime_noise_ipc::DenoiseMode::Bypass,
        DenoiseMode::Mute => realtime_noise_ipc::DenoiseMode::Mute,
    }
}

/// Converts IPC `StudioPreset` to `studio_dsp::Preset`.
#[must_use]
pub const fn convert_ipc_preset_to_dsp(preset: realtime_noise_ipc::StudioPreset) -> Preset {
    match preset {
        realtime_noise_ipc::StudioPreset::Off => Preset::Off,
        realtime_noise_ipc::StudioPreset::Natural => Preset::Natural,
        realtime_noise_ipc::StudioPreset::Podcast => Preset::Podcast,
        realtime_noise_ipc::StudioPreset::Broadcast => Preset::Broadcast,
    }
}

/// Converts `studio_dsp::Preset` to IPC `StudioPreset`.
#[must_use]
pub const fn convert_dsp_preset_to_ipc(preset: Preset) -> realtime_noise_ipc::StudioPreset {
    match preset {
        Preset::Off => realtime_noise_ipc::StudioPreset::Off,
        Preset::Natural => realtime_noise_ipc::StudioPreset::Natural,
        Preset::Podcast => realtime_noise_ipc::StudioPreset::Podcast,
        Preset::Broadcast => realtime_noise_ipc::StudioPreset::Broadcast,
    }
}
