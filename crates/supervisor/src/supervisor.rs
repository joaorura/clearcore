#![forbid(unsafe_code)]

use crate::backend::{
    AUTO_QUALIFICATION_MAX_HOP_MS, BackendResolutionInfo, instantiate_backend_with_fallback,
    is_explicit_accelerator_request, openvino_devices_for_request, qualify_backend_runtime,
};
use crate::backoff::{BackoffTracker, MAX_CRASHES_PER_15_MINUTES};
use realtime_noise_accelerators::OpenVINOBackend;
use realtime_noise_contracts::{AudioFrame, HOP_SAMPLES};
use realtime_noise_engine::DenoiseMode;
use realtime_noise_model::{
    BackendDescriptor, CpuProfile, InferenceBackend, InferenceError, PDFNET3_DEV_ASSET_ID,
    PDFNET3_PRO_ASSET_ID, PdfNet3DevArchive, SpeakerConditioning, StudioBackend, StudioResetHandle,
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
    /// Speaker conditioning (pDFNet3 Pro v3 full) confirmed by the live backend.
    active_conditioning: Option<SpeakerConditioning>,
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
    /// Where the `FiLM` stateful export `OpenVINO` runs for a `FiLM` (v2) isolation model comes
    /// from. Only a development override can be a `FiLM` export now: the bundled model is the Pro
    /// v3 full manifest export, which `OpenVINO` runs from the isolation archive itself, so `None`
    /// means "no `FiLM` stateful export" (`MODEL_ASSET_NOT_FOUND`).
    stateful_source: Option<StatefulSource>,
    /// The verified stateful export once loaded (or the `MODEL_*` code of why it is not).
    stateful_archive: Option<Result<PdfNet3DevArchive, &'static str>>,
    /// Resolution of the last [`Self::select_backend`] (`GetStatus` reports its fallback reason).
    last_resolution: Option<BackendResolutionInfo>,
}

/// Loader of the pDFNet3 stateful export, injectable for tests.
pub type StatefulSource = Box<dyn Fn() -> Result<PdfNet3DevArchive, &'static str> + Send + Sync>;

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
            active_conditioning: None,
            dev_base_model: None,
            dev_base_model_error: None,
            isolation_model_required: false,
            isolation_model_error: None,
            stateful_source: None,
            stateful_archive: None,
            last_resolution: None,
        }
    }

    /// Replaces the source of the pDFNet3 stateful export used by the `OpenVINO` backends (tests
    /// and development); the cached archive is dropped.
    pub fn set_stateful_source(&mut self, source: Option<StatefulSource>) {
        self.stateful_source = source;
        self.stateful_archive = None;
    }

    /// Resolution of the last backend selection (name, device, fallback and its reason).
    #[must_use]
    pub const fn last_resolution(&self) -> Option<&BackendResolutionInfo> {
        self.last_resolution.as_ref()
    }

    /// The verified pDFNet3 stateful export, loaded once (fixed `MODEL_*` code on failure).
    fn stateful_archive(&mut self) -> Result<PdfNet3DevArchive, &'static str> {
        if self.stateful_archive.is_none() {
            let loaded = self.stateful_source.as_ref().map_or(
                Err(realtime_noise_model::pdfnet3_bundle::MODEL_ASSET_NOT_FOUND),
                |source| source(),
            );
            self.stateful_archive = Some(loaded);
        }
        self.stateful_archive
            .clone()
            .unwrap_or(Err("MODEL_UNAVAILABLE"))
    }

    /// Compiles the pDFNet3 Pro stateful export on the first of `devices` that loads and meets
    /// the real-time deadline. The error is the reason shown as `fallback_reason`.
    fn instantiate_openvino_pdfnet3(
        &mut self,
        devices: &[String],
    ) -> Result<(OpenVINOBackend, BackendResolutionInfo), String> {
        let probe = realtime_noise_runtime_openvino::probe_runtime_cached();
        if !probe.state.is_usable() {
            return Err(format!(
                "OpenVINO indisponível ({}): {}",
                probe.state.code(),
                probe.message
            ));
        }
        if devices.is_empty() {
            return Err("OpenVINO não listou nenhum dispositivo".to_owned());
        }
        let archive = self
            .stateful_archive()
            .map_err(|code| format!("export stateful do pDFNet3 Pro indisponível ({code})"))?;
        let mut failures = Vec::new();
        for device in devices {
            let mut backend = match OpenVINOBackend::load_pdfnet3(&archive, device) {
                Ok(backend) => backend,
                Err(error) => {
                    failures.push(format!("{device}: {error}"));
                    continue;
                }
            };
            if !qualify_backend_runtime(&mut backend, AUTO_QUALIFICATION_MAX_HOP_MS) {
                failures.push(format!(
                    "{device}: excedeu o prazo de tempo real de {AUTO_QUALIFICATION_MAX_HOP_MS:.1} ms por hop"
                ));
                continue;
            }
            // Qualification ran silence through the network: start the stream clean.
            if let Err(error) = backend.reset() {
                failures.push(format!("{device}: {error}"));
                continue;
            }
            let device = backend.device().to_owned();
            let descriptor = backend.descriptor();
            let info = BackendResolutionInfo {
                name: format!("openvino-{}", device.to_ascii_lowercase()),
                runtime: descriptor.runtime.to_owned(),
                device,
                is_hardware_accelerated: true,
                is_fallback: false,
                fallback_reason: None,
            };
            return Ok((backend, info));
        }
        Err(format!("OpenVINO falhou: {}", failures.join("; ")))
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
        self.reapply_speaker_conditioning();
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
        self.active_conditioning = None;
        Ok(())
    }

    /// Applies the pDFNet3 Pro v3 full speaker conditioning (embedding [192] + EQ + intensity, the
    /// service's `VoiceConditioning`) to the live backend, or returns to the neutral voice with
    /// `None`. Reported as active only after the backend confirms it; on failure the previous
    /// conditioning stays. It replaces a legacy profile and is re-applied on backend switches.
    pub fn apply_speaker_conditioning(
        &mut self,
        conditioning: Option<&SpeakerConditioning>,
    ) -> Result<(), InferenceError> {
        let backend = self
            .backend
            .as_mut()
            .ok_or_else(|| InferenceError::UnsupportedFeature("no active backend".into()))?;
        backend.set_speaker_conditioning(conditioning)?;
        self.active_conditioning = conditioning.cloned();
        self.active_profile = None;
        Ok(())
    }

    /// Whether the live backend takes a speaker embedding (pDFNet3 Pro v3 full graphs).
    #[must_use]
    pub fn supports_speaker_embedding(&self) -> bool {
        self.backend
            .as_deref()
            .is_some_and(InferenceBackend::supports_speaker_embedding)
    }

    #[must_use]
    pub const fn active_speaker_conditioning(&self) -> Option<&SpeakerConditioning> {
        self.active_conditioning.as_ref()
    }

    /// Profile id of the applied speaker conditioning.
    #[must_use]
    pub fn active_conditioning_id(&self) -> Option<&str> {
        self.active_conditioning
            .as_ref()
            .map(|c| c.profile_id.as_str())
    }

    /// Whether the applied speaker conditioning carries a spectral EQ.
    #[must_use]
    pub fn active_conditioning_has_eq(&self) -> bool {
        self.active_conditioning
            .as_ref()
            .is_some_and(|c| c.eq.is_some())
    }

    /// Re-applies the stored conditioning to a freshly installed backend; dropped if refused.
    fn reapply_speaker_conditioning(&mut self) {
        if let Some(conditioning) = self.active_conditioning.take()
            && self
                .apply_speaker_conditioning(Some(&conditioning))
                .is_err()
        {
            self.active_conditioning = None;
        }
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
        // The pDFNet3 (FiLM) overrides EVERY request. The bundled Pro runs on OpenVINO (its
        // stateful export, FiLM inputs fed) when an OpenVINO backend is requested explicitly and
        // works; everything else, and every OpenVINO failure, runs the tract export on the CPU.
        // `auto` stays on tract: see docs/accelerators (the virtual microphone's filter runs ONNX
        // Runtime on the CPU either way, and tract is the parity reference).
        if let Some(archive) = self.dev_base_model.clone() {
            let mut openvino_failure = None;
            if let Some(devices) = openvino_devices_for_request(request) {
                if archive.is_manifest_export() {
                    // One export for every runtime: the same archive runs on OpenVINO.
                    match instantiate_openvino_stateful(&archive, &devices) {
                        Ok((backend, info)) => {
                            self.dev_base_model_error = None;
                            self.isolation_model_error = None;
                            self.set_backend(backend, &info.name);
                            self.last_resolution = Some(info.clone());
                            return info;
                        }
                        Err(reason) => {
                            eprintln!("pDFNet3 Pro v3 on OpenVINO unavailable: {reason}");
                            openvino_failure = Some(reason);
                        }
                    }
                } else if archive.asset_id() == PDFNET3_PRO_ASSET_ID {
                    match self.instantiate_openvino_pdfnet3(&devices) {
                        Ok((backend, info)) => {
                            self.dev_base_model_error = None;
                            self.isolation_model_error = None;
                            self.set_backend(Box::new(backend), &info.name);
                            self.last_resolution = Some(info.clone());
                            return info;
                        }
                        Err(reason) => {
                            eprintln!("pDFNet3 Pro on OpenVINO unavailable: {reason}");
                            openvino_failure = Some(reason);
                        }
                    }
                } else {
                    openvino_failure = Some(
                        "o pDFNet3 de desenvolvimento (CLEARCORE_DEV_PDFNET3_*) roda só no tract"
                            .to_owned(),
                    );
                }
            }
            match archive.instantiate(CpuProfile::Avx2Minimum) {
                Ok(backend) => {
                    self.dev_base_model_error = None;
                    self.isolation_model_error = None;
                    let accelerator = is_explicit_accelerator_request(request);
                    let fallback_reason = match openvino_failure {
                        Some(reason) => Some(format!("pDFNet3 Pro no tract (CPU): {reason}")),
                        None if accelerator => Some(
                            "The pDFNet3 Pro isolation model runs on the CPU (tract): this \
                             accelerator backend cannot feed its FiLM voice-profile inputs"
                                .to_string(),
                        ),
                        None => None,
                    };
                    let info = BackendResolutionInfo {
                        name: "tract".to_string(),
                        runtime: "tract".to_string(),
                        device: "CPU".to_string(),
                        is_hardware_accelerated: false,
                        is_fallback: accelerator,
                        fallback_reason,
                    };
                    self.set_backend(Box::new(backend), &info.name);
                    self.last_resolution = Some(info.clone());
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
            let info = self.clear_backend_for_missing_model();
            self.last_resolution = Some(info.clone());
            return info;
        }
        let (backend, info) = instantiate_backend_with_fallback(request, model_dir, repo_root);
        self.set_backend(backend, &info.name);
        self.last_resolution = Some(info.clone());
        info
    }

    /// Production policy without its model: no backend at all (never the base `DFNet3`), the
    /// applied profile is dropped and the resolution names the fixed `MODEL_*` code.
    fn clear_backend_for_missing_model(&mut self) -> BackendResolutionInfo {
        let code = self.isolation_model_error.unwrap_or("MODEL_UNAVAILABLE");
        self.backend = None;
        self.active_backend_name.clear();
        self.active_profile = None;
        self.active_conditioning = None;
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

/// `BackendDescriptor` of the generic stateful executor on `OpenVINO` (`backend = "openvino"`,
/// `runtime = "openvino-<device>"`, like the legacy `OpenVINOBackend`, so status and device
/// reporting stay the same).
fn openvino_stateful_descriptor(archive: &PdfNet3DevArchive, device: &str) -> BackendDescriptor {
    let (runtime, cpu_profile) = match device.split('.').next().unwrap_or(device) {
        "GPU" => ("openvino-gpu", "intel-gpu"),
        "CPU" => ("openvino-cpu", "intel-cpu"),
        _ => ("openvino-npu", "intel-npu"),
    };
    let mut descriptor =
        archive.descriptor("openvino", runtime, "2025/2026", CpuProfile::Avx2Minimum);
    descriptor.cpu_profile = cpu_profile;
    descriptor
}

/// Compiles the manifest export (pDFNet3 Pro v3 full) with the generic stateful executor on the
/// first of `devices` that loads and meets the real-time deadline (then resets it). The error is
/// the reason shown as `fallback_reason`.
fn instantiate_openvino_stateful(
    archive: &PdfNet3DevArchive,
    devices: &[String],
) -> Result<(Box<dyn InferenceBackend>, BackendResolutionInfo), String> {
    use realtime_noise_model::StatefulGraphBackend;
    let probe = realtime_noise_runtime_openvino::probe_runtime_cached();
    if !probe.state.is_usable() {
        return Err(format!(
            "OpenVINO indisponível ({}): {}",
            probe.state.code(),
            probe.message
        ));
    }
    if devices.is_empty() {
        return Err("OpenVINO não listou nenhum dispositivo".to_owned());
    }
    let files = archive
        .stateful_files()
        .map_err(|error| format!("export do pDFNet3 Pro v3 inválido ({})", error.model_code()))?;
    let mut failures = Vec::new();
    for device in devices {
        let built = realtime_noise_runtime_openvino::build_openvino_executor(&files, device)
            .and_then(|executor| {
                StatefulGraphBackend::new(executor, openvino_stateful_descriptor(archive, device))
            });
        let mut backend = match built {
            Ok(backend) => backend,
            Err(error) => {
                failures.push(format!("{device}: {error}"));
                continue;
            }
        };
        if !backend.supports_speaker_embedding() {
            failures.push(format!("{device}: o grafo não declara speaker_embedding"));
            continue;
        }
        if !qualify_backend_runtime(&mut backend, AUTO_QUALIFICATION_MAX_HOP_MS) {
            failures.push(format!(
                "{device}: excedeu o prazo de tempo real de {AUTO_QUALIFICATION_MAX_HOP_MS:.1} ms por hop"
            ));
            continue;
        }
        if let Err(error) = backend.reset() {
            failures.push(format!("{device}: {error}"));
            continue;
        }
        let info = BackendResolutionInfo {
            name: format!("openvino-{}", device.to_ascii_lowercase()),
            runtime: InferenceBackend::descriptor(&backend).runtime.to_owned(),
            device: device.clone(),
            is_hardware_accelerated: true,
            is_fallback: false,
            fallback_reason: None,
        };
        return Ok((Box::new(backend), info));
    }
    Err(format!("OpenVINO falhou: {}", failures.join("; ")))
}
