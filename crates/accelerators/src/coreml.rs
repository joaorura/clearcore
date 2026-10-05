#![forbid(unsafe_code)]

use std::path::Path;

use realtime_noise_contracts::AudioFrame;
use realtime_noise_model::{
    ALGORITHM_LATENCY_SAMPLES, ApprovedAssetManifest, BackendDescriptor, InferenceBackend,
    InferenceError, ProcessedFrame,
};
use realtime_noise_runtime_coreml::{ComputeUnit, CoreMlModelRunner};

/// `CoreML` acceleration backend targeting Apple Silicon ANE / GPU devices.
///
/// Only [`Self::load_compiled`] on macOS evaluates a real model; every other constructor yields a
/// simulated runner that copies frames through (see [`Self::executes_inference`]).
#[derive(Debug, Clone)]
pub struct CoreMlBackend {
    descriptor: BackendDescriptor,
    runner: CoreMlModelRunner,
    compute_unit: String,
    simulated_failure: bool,
}

impl CoreMlBackend {
    /// Creates a new `CoreMlBackend` with the specified asset metadata and target compute unit.
    pub fn new(
        asset_id: impl Into<String>,
        asset_sha256: impl Into<String>,
        compute_unit: impl Into<String>,
    ) -> Self {
        let cu_str: String = compute_unit.into();
        let parsed_cu = ComputeUnit::from_str_name(&cu_str);
        let id_str: String = asset_id.into();
        let sha_str: String = asset_sha256.into();

        let runner = CoreMlModelRunner::new_simulated(&id_str, parsed_cu);

        Self {
            descriptor: BackendDescriptor {
                backend: "coreml",
                backend_version: "1.0",
                runtime: "coreml-ane",
                runtime_version: "17.0",
                asset_id: id_str,
                asset_sha256: sha_str,
                cpu_profile: "apple-silicon-ane",
            },
            runner,
            compute_unit: cu_str,
            simulated_failure: false,
        }
    }

    /// Loads a compiled `.mlmodelc` bundle from disk into a live hardware-accelerated backend.
    pub fn load_compiled(
        model_path: impl AsRef<Path>,
        compute_unit: impl Into<String>,
    ) -> Result<Self, InferenceError> {
        let cu_str: String = compute_unit.into();
        let parsed_cu = ComputeUnit::from_str_name(&cu_str);
        let path = model_path.as_ref();
        let runner = CoreMlModelRunner::load(path, parsed_cu)?;

        let asset_id = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("coreml-model")
            .to_owned();

        Ok(Self {
            descriptor: BackendDescriptor {
                backend: "coreml",
                backend_version: "1.0",
                runtime: "coreml-ane",
                runtime_version: "17.0",
                asset_id,
                asset_sha256: "compiled-mlmodelc-sha256".to_owned(),
                cpu_profile: "apple-silicon-ane",
            },
            runner,
            compute_unit: cu_str,
            simulated_failure: false,
        })
    }

    /// Creates a `CoreMlBackend` directly from an approved asset manifest.
    #[must_use]
    pub fn from_manifest(manifest: &ApprovedAssetManifest) -> Self {
        Self::new(manifest.asset_id(), manifest.asset_sha256(), "all")
    }

    /// Creates a simulated mock backend for unit testing and CI pipelines.
    #[must_use]
    pub fn new_mock() -> Self {
        Self::new("df-compatible-release-asset-v1", "mock-asset-sha256", "all")
    }

    /// Creates a simulated mock backend with an explicit compute unit.
    #[must_use]
    pub fn new_mock_with_unit(compute_unit: impl Into<String>) -> Self {
        Self::new(
            "df-compatible-release-asset-v1",
            "mock-asset-sha256",
            compute_unit,
        )
    }

    /// Name of the configured compute unit.
    #[must_use]
    pub fn compute_unit(&self) -> &str {
        &self.compute_unit
    }

    /// Parsed [`ComputeUnit`] enum value.
    #[must_use]
    pub const fn parsed_compute_unit(&self) -> ComputeUnit {
        self.runner.compute_unit()
    }

    /// Reference to the underlying [`CoreMlModelRunner`].
    #[must_use]
    pub const fn runner(&self) -> &CoreMlModelRunner {
        &self.runner
    }

    /// Mutable reference to the underlying [`CoreMlModelRunner`].
    pub const fn runner_mut(&mut self) -> &mut CoreMlModelRunner {
        &mut self.runner
    }

    /// Whether frames are evaluated by a loaded `CoreML` model. `false` for simulated backends
    /// (including everything built by [`Self::new`]) and on platforms without `CoreML`, where
    /// frames are copied through unchanged.
    #[must_use]
    pub fn executes_inference(&self) -> bool {
        self.runner.executes_inference()
    }

    /// Whether a loaded model runs on Apple Silicon hardware (ANE / GPU). Implies
    /// [`Self::executes_inference`].
    #[must_use]
    pub fn is_hardware_accelerated(&self) -> bool {
        self.runner.is_hardware_accelerated()
    }

    /// Configures simulated failure mode.
    pub fn set_simulated_failure(&mut self, fail: bool) {
        self.simulated_failure = fail;
        self.runner.set_simulated_failure(fail);
    }
}

impl InferenceBackend for CoreMlBackend {
    fn descriptor(&self) -> BackendDescriptor {
        self.descriptor.clone()
    }

    fn process(&mut self, input: &AudioFrame) -> Result<ProcessedFrame, InferenceError> {
        if self.simulated_failure {
            return Err(InferenceError::InferenceExecution(
                "CoreML execution failed".to_owned(),
            ));
        }

        if input.iter().any(|sample| !sample.is_finite()) {
            return Err(InferenceError::InputContract(
                "input frame contains a non-finite sample".to_owned(),
            ));
        }

        let output = self.runner.run_frame(input)?;
        ProcessedFrame::checked(output, ALGORITHM_LATENCY_SAMPLES, self.descriptor())
    }

    fn algorithmic_latency_samples(&self) -> u32 {
        ALGORITHM_LATENCY_SAMPLES
    }

    fn set_voice_profile(
        &mut self,
        profile: Option<&realtime_noise_model::VoiceProfile>,
    ) -> Result<(), realtime_noise_model::InferenceError> {
        realtime_noise_model::reject_unsupported_voice_profile(profile)
    }

    fn supports_voice_profile(&self) -> bool {
        false
    }
}
