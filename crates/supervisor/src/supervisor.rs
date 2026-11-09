#![forbid(unsafe_code)]

use crate::backend::{
    BackendResolutionInfo, instantiate_backend_with_fallback, is_explicit_accelerator_request,
};
use crate::backoff::{BackoffTracker, MAX_CRASHES_PER_15_MINUTES};
use realtime_noise_contracts::{AudioFrame, HOP_SAMPLES};
use realtime_noise_engine::DenoiseMode;
use realtime_noise_model::{
    BackendDescriptor, CpuProfile, InferenceBackend, InferenceError, PDFNET3_DEV_ASSET_ID,
    PDFNET3_PRO_ASSET_ID, PdfNet3DevArchive, StudioBackend, StudioResetHandle, VoiceProfile,
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
    /// pDFNet3 archive (`FiLM`) that overrides every backend selection while set: the bundled
    /// pDFNet3 Pro in production, or the development override.
    dev_base_model: Option<PdfNet3DevArchive>,
    /// Fixed code of the last reason the development model could not be used.
    dev_base_model_error: Option<&'static str>,
    /// Production policy ([`Self::require_isolation_model`]): the pDFNet3 is the ONLY isolation
    /// model. Without it no backend is installed (no fallback to the base `DFNet3`).
    isolation_model_required: bool,
    /// Fixed `MODEL_*` code while the required isolation model is unavailable.
    isolation_model_error: Option<&'static str>,
}

/// `GetStatus.isolation_model` while the bundled pDFNet3 Pro drives the live backend.
pub const ISOLATION_MODEL_PDFNET3_PRO: &str = "pdfnet3-pro";
/// `GetStatus.isolation_model` while the development pDFNet3 override drives the live backend.
pub const ISOLATION_MODEL_PDFNET3_DEV: &str = "pdfnet3-dev";
/// `GetStatus.isolation_model` for any other live model (legacy/test seams only).
pub const ISOLATION_MODEL_BASE: &str = "base";
/// `GetStatus.isolation_model` when no isolation model is loaded.
pub const ISOLATION_MODEL_NONE: &str = "none";

/// `GetStatus.dev_base_model` while the development pDFNet3 drives the live backend.
pub const DEV_BASE_MODEL_PDFNET3: &str = "pdfnet3-dev";
/// `GetStatus.dev_base_model` otherwise (the default base model, or no model at all).
pub const DEV_BASE_MODEL_BASE: &str = "base";

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
            dev_base_model: None,
            dev_base_model_error: None,
            isolation_model_required: false,
            isolation_model_error: None,
        }
    }

    /// Production policy: `model` (the verified pDFNet3 archive, or the fixed `MODEL_*` code of
    /// why it is unavailable) becomes the ONLY isolation model. Every backend selection installs
    /// tract with it; when it is unavailable no backend is installed, the status reports the code
    /// and nothing falls back to the base `DFNet3`. Takes effect on the next selection.
    pub fn require_isolation_model(&mut self, model: Result<PdfNet3DevArchive, &'static str>) {
        self.isolation_model_required = true;
        match model {
            Ok(archive) => {
                self.dev_base_model = Some(archive);
                self.isolation_model_error = None;
            }
            Err(code) => {
                self.dev_base_model = None;
                self.isolation_model_error = Some(code);
            }
        }
    }

    /// The verified pDFNet3 archive, if one is configured (shared with the enrollment denoiser).
    #[must_use]
    pub const fn isolation_archive(&self) -> Option<&PdfNet3DevArchive> {
        self.dev_base_model.as_ref()
    }

    /// Whether the production "pDFNet3 only" policy is active.
    #[must_use]
    pub const fn isolation_model_required(&self) -> bool {
        self.isolation_model_required
    }

    /// Fixed `MODEL_*` code while the required isolation model is unavailable.
    #[must_use]
    pub const fn isolation_model_error(&self) -> Option<&'static str> {
        self.isolation_model_error
    }

    /// Which isolation model the live backend runs: [`ISOLATION_MODEL_PDFNET3_PRO`],
    /// [`ISOLATION_MODEL_PDFNET3_DEV`], [`ISOLATION_MODEL_BASE`] or [`ISOLATION_MODEL_NONE`].
    #[must_use]
    pub fn isolation_model(&self) -> &'static str {
        match self.active_backend_descriptor() {
            None => ISOLATION_MODEL_NONE,
            Some(desc) if desc.asset_id == PDFNET3_PRO_ASSET_ID => ISOLATION_MODEL_PDFNET3_PRO,
            Some(desc) if desc.asset_id == PDFNET3_DEV_ASSET_ID => ISOLATION_MODEL_PDFNET3_DEV,
            Some(_) => ISOLATION_MODEL_BASE,
        }
    }

    /// Configures the development pDFNet3 base model (already hash- and member-verified). While
    /// set, [`Self::select_backend`] installs tract with this model whatever was requested
    /// (`auto` would otherwise pick an accelerator that can never apply a voice profile). If the
    /// model fails to load, the error code is recorded and the normal selection runs: the
    /// daemon never switches models silently, the status says which one is live and why.
    /// Takes effect on the next selection.
    pub fn set_dev_base_model(&mut self, archive: Option<PdfNet3DevArchive>) {
        self.dev_base_model = archive;
        self.dev_base_model_error = None;
    }

    /// Records why the development model is not in use (configuration/verification failure
    /// found before it reached the supervisor). Fixed codes only, never a path.
    pub fn set_dev_base_model_error(&mut self, code: Option<&'static str>) {
        self.dev_base_model_error = code;
    }

    #[must_use]
    pub const fn dev_base_model_error(&self) -> Option<&'static str> {
        self.dev_base_model_error
    }

    /// [`DEV_BASE_MODEL_PDFNET3`] when the live backend runs the development pDFNet3,
    /// [`DEV_BASE_MODEL_BASE`] otherwise.
    #[must_use]
    pub fn dev_base_model(&self) -> &'static str {
        if self
            .active_backend_descriptor()
            .is_some_and(|desc| desc.asset_id == PDFNET3_DEV_ASSET_ID)
        {
            DEV_BASE_MODEL_PDFNET3
        } else {
            DEV_BASE_MODEL_BASE
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
    pub fn voice_leveler_intensity(&self) -> u8 {
        self.studio_control.leveler_intensity()
    }

    pub fn set_voice_leveler_intensity(&mut self, intensity: u8) {
        self.studio_control.set_leveler_intensity(intensity);
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
            desc.runtime_version != "mock"
                && desc.runtime_version != "unknown"
                && ((desc.backend == "openvino" && desc.runtime != "openvino-cpu")
                    || desc.backend == "tensorrt")
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
            } else if desc.backend == "tensorrt" {
                "GPU".to_string()
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
    /// backend confirms it (`FiLM` inputs); on failure the previously active profile is preserved.
    /// There is no "DSP bridge": a backend without `FiLM` refuses a conditioned profile, which is
    /// exactly what [`Self::supports_voice_profile`] announces up front.
    pub fn set_voice_profile(
        &mut self,
        profile: Option<&VoiceProfile>,
    ) -> Result<(), InferenceError> {
        let backend = self
            .backend
            .as_mut()
            .ok_or_else(|| InferenceError::UnsupportedFeature("no active backend".into()))?;

        let Some(p) = profile else {
            backend.set_voice_profile(None)?;
            self.active_profile = None;
            return Ok(());
        };

        p.verify_integrity()
            .map_err(|e| InferenceError::InputContract(e.to_string()))?;

        backend.set_voice_profile(Some(p))?;
        self.active_profile = Some(p.clone());
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

    /// Whether the live backend can apply a conditioned voice profile: exactly when its model
    /// declares the `FiLM` inputs, so `true` here means [`Self::set_voice_profile`] accepts a
    /// conditioned profile and `false` lets `BuildVoiceProfile` refuse up front.
    #[must_use]
    pub fn supports_voice_profile(&self) -> bool {
        self.backend
            .as_deref()
            .is_some_and(InferenceBackend::supports_voice_profile)
    }

    /// Re-applies the stored profile to a freshly installed backend. If the new backend does not
    /// confirm it, the profile is dropped so an unconfirmed profile is never reported.
    fn reapply_voice_profile(&mut self) {
        if let Some(profile) = self.active_profile.take()
            && self.set_voice_profile(Some(&profile)).is_err()
        {
            self.active_profile = None;
        }
    }

    pub fn select_backend(
        &mut self,
        request: &str,
        model_dir: Option<&Path>,
        repo_root: Option<&Path>,
    ) -> BackendResolutionInfo {
        self.requested_backend_name = request.to_string();
        // The pDFNet3 (FiLM) overrides EVERY request: its graphs only run on tract here, and an
        // accelerator would have to run another model, which could never apply a voice profile.
        if let Some(archive) = &self.dev_base_model {
            match archive.instantiate(CpuProfile::Avx2Minimum) {
                Ok(backend) => {
                    self.dev_base_model_error = None;
                    self.isolation_model_error = None;
                    let accelerator = is_explicit_accelerator_request(request);
                    let info = BackendResolutionInfo {
                        name: "tract".to_string(),
                        runtime: "tract".to_string(),
                        device: "CPU".to_string(),
                        is_hardware_accelerated: false,
                        is_fallback: accelerator,
                        fallback_reason: accelerator.then(|| {
                            "The pDFNet3 Pro isolation model runs on the CPU (tract): the \
                             accelerator backends cannot feed its FiLM voice-profile inputs"
                                .to_string()
                        }),
                    };
                    self.set_backend(Box::new(backend), &info.name);
                    return info;
                }
                Err(error) => {
                    if self.isolation_model_required {
                        eprintln!(
                            "pDFNet3 isolation model failed to load ({}); no isolation backend",
                            error.model_code()
                        );
                        self.isolation_model_error = Some(error.model_code());
                    } else {
                        eprintln!(
                            "Development pDFNet3 model was not loaded ({}); using the default model",
                            error.code()
                        );
                        self.dev_base_model_error = Some(error.code());
                    }
                }
            }
        }
        if self.isolation_model_required {
            return self.clear_backend_for_missing_model();
        }
        let (backend, info) = instantiate_backend_with_fallback(request, model_dir, repo_root);
        self.set_backend(backend, &info.name);
        info
    }

    /// Production policy without its model: no backend at all (never the base `DFNet3`), the
    /// applied profile is dropped and the resolution names the fixed `MODEL_*` code.
    fn clear_backend_for_missing_model(&mut self) -> BackendResolutionInfo {
        let code = self.isolation_model_error.unwrap_or("MODEL_UNAVAILABLE");
        self.backend = None;
        self.active_backend_name.clear();
        self.active_profile = None;
        BackendResolutionInfo {
            name: "none".to_string(),
            runtime: "none".to_string(),
            device: "none".to_string(),
            is_hardware_accelerated: false,
            is_fallback: true,
            fallback_reason: Some(format!(
                "The pDFNet3 Pro isolation model is unavailable ({code}); no fallback model is used"
            )),
        }
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
