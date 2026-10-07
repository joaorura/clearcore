#![forbid(unsafe_code)]
#![allow(clippy::doc_markdown)]

//! NVIDIA `TensorRT` acceleration backend.
//!
//! # Wiring
//!
//! - **Detection**: [`TensorRtBackend::is_available`] loads the CUDA driver and `libnvinfer`
//!   dynamically through `realtime-noise-runtime-tensorrt`; hosts without the NVIDIA stack just
//!   see `false`. [`crate::HardwareScanner`] uses the same discovery for its audit.
//! - **Loading**: [`TensorRtBackend::try_new_hardware`] opens the CUDA context, stream and VRAM
//!   buffers on one GPU. [`TensorRtBackend::new`] does the same but degrades to a mock context
//!   (see [`TensorRtBackend::fallback_reason`]) when the stack is missing, mirroring the other
//!   runtimes. [`TensorRtBackend::load_stateful`] loads the compiled TensorRT engines
//!   (`enc.engine`, `erb_dec.engine`, `df_dec.engine`) and wires the real DeepFilterNet3 DSP pipeline.
//! - **Precision**: the Clearcore policy is applied per GPU by
//!   `GpuDevice::preferred_precision`: FP8 on `sm_89` and later, else FP16, else INT8. FP32 is
//!   never chosen for a GPU, so a GPU supporting none of those yields an error and the caller
//!   falls back to another backend.
//! - **Strictly TensorRT**: raw CUDA execution is excluded; there is no `CudaBackend`.

use std::path::Path;

use deep_filter::Complex32;
use realtime_noise_contracts::{AudioFrame, HOP_SAMPLES};
use realtime_noise_model::{
    ALGORITHM_LATENCY_SAMPLES, AgnosticDspBackend, ApprovedAssetManifest, BackendDescriptor,
    DspPipelineConfig, ExtractedFeatures, InferenceBackend, InferenceError, ModelFrameOutput,
    ProcessedFrame, SpectralModelBackend,
};
use realtime_noise_runtime_tensorrt::dfn3::{DF_COEFS_LEN, DF_ORDER, NB_DF};
use realtime_noise_runtime_tensorrt::{
    CudaDevice, Dfn3Output, PrecisionTarget, TensorRtDfn3Session, TensorRtError,
    TensorRtInferenceContext, TensorRtLibrary, TensorRtRuntime,
};

const DEFAULT_ASSET_ID: &str = "df-compatible-release-asset-v1";
const MOCK_ASSET_SHA256: &str = "mock-asset-sha256";

/// Static label for the GPU architecture, as carried by [`BackendDescriptor::cpu_profile`].
fn profile_for_sm(sm: &str) -> &'static str {
    match sm {
        "sm_61" => "sm_61",
        "sm_70" => "sm_70",
        "sm_75" => "sm_75",
        "sm_80" => "sm_80",
        "sm_86" => "sm_86",
        "sm_87" => "sm_87",
        "sm_89" => "sm_89",
        "sm_90" => "sm_90",
        "sm_100" => "sm_100",
        "sm_103" => "sm_103",
        "sm_120" => "sm_120",
        "sm_121" => "sm_121",
        _ => "nvidia-gpu",
    }
}

fn execution_error(error: &TensorRtError) -> InferenceError {
    InferenceError::InferenceExecution(error.to_string())
}

/// Adapter running a [`TensorRtDfn3Session`] as the neural stage of the shared DSP pipeline.
struct TensorRtSpectralModel {
    session: TensorRtDfn3Session,
    descriptor: BackendDescriptor,
    scratch: Box<Dfn3Output>,
}

impl SpectralModelBackend for TensorRtSpectralModel {
    fn descriptor(&self) -> BackendDescriptor {
        self.descriptor.clone()
    }

    fn process_spectrum(
        &mut self,
        features: &ExtractedFeatures,
    ) -> Result<ModelFrameOutput, InferenceError> {
        self.session
            .run_frame(&features.feat_erb, &features.feat_spec, &mut self.scratch)
            .map_err(|error| execution_error(&error))?;

        // `df_coefs` is `[bin][tap][re, im]` flattened.
        let df_coefs: Vec<[Complex32; DF_ORDER]> = self
            .scratch
            .df_coefs
            .chunks_exact(DF_COEFS_LEN / NB_DF)
            .map(|bin| std::array::from_fn(|tap| Complex32::new(bin[2 * tap], bin[2 * tap + 1])))
            .collect();

        Ok(ModelFrameOutput {
            lsnr: self.scratch.lsnr,
            erb_gains: Some(self.scratch.erb_mask),
            df_coefs: Some(df_coefs),
        })
    }

    fn reset(&mut self) -> Result<(), InferenceError> {
        self.session
            .reset()
            .map_err(|error| execution_error(&error))
    }
}

enum Engine {
    /// Passthrough or CUDA round-trip context (stub / mock).
    Passthrough(Box<TensorRtInferenceContext>),
    /// Real TensorRT engine execution inside the shared DSP pipeline.
    Neural(Box<AgnosticDspBackend<TensorRtSpectralModel>>),
}

/// NVIDIA `TensorRT` acceleration backend targeting NVIDIA GPUs.
/// Strictly uses `TensorRT`; raw CUDA execution is excluded.
pub struct TensorRtBackend {
    descriptor: BackendDescriptor,
    device_id: u32,
    device_name: String,
    sm_version: String,
    precision: PrecisionTarget,
    engine: Engine,
    fallback_reason: Option<String>,
    simulated_failure: bool,
    native_version: Option<String>,
}

impl std::fmt::Debug for TensorRtBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TensorRtBackend")
            .field("descriptor", &self.descriptor)
            .field("device_id", &self.device_id)
            .field("device_name", &self.device_name)
            .field("sm_version", &self.sm_version)
            .field("precision", &self.precision)
            .field("executes_engine", &self.executes_engine())
            .field("fallback_reason", &self.fallback_reason)
            .field("simulated_failure", &self.simulated_failure)
            .finish_non_exhaustive()
    }
}

impl TensorRtBackend {
    fn descriptor_for_sm(
        sm: &str,
        asset_id: String,
        asset_sha256: String,
    ) -> BackendDescriptor {
        BackendDescriptor {
            backend: "tensorrt",
            backend_version: env!("CARGO_PKG_VERSION"),
            runtime: "tensorrt",
            runtime_version: "unknown",
            asset_id,
            asset_sha256,
            cpu_profile: profile_for_sm(sm),
        }
    }

    fn from_context(
        context: TensorRtInferenceContext,
        device_id: u32,
        asset_id: String,
        asset_sha256: String,
        fallback_reason: Option<String>,
    ) -> Self {
        let sm = context.device().sm_string();
        let device_name = context.device().name().to_owned();
        let precision = context.precision();
        let native_version = context.is_hardware_backed().then(|| context.runtime_version().to_owned());
        Self {
            descriptor: Self::descriptor_for_sm(&sm, asset_id, asset_sha256),
            device_id,
            device_name,
            sm_version: sm,
            precision,
            engine: Engine::Passthrough(Box::new(context)),
            fallback_reason,
            simulated_failure: false,
            native_version,
        }
    }

    /// Opens a real CUDA/TensorRT context on `device_id`, choosing the precision with the
    /// Clearcore policy. Fails when the NVIDIA stack is missing or the GPU supports none of
    /// FP8/FP16/INT8, so callers can fall back to another backend.
    pub fn try_new_hardware(
        asset_id: impl Into<String>,
        asset_sha256: impl Into<String>,
        device_id: u32,
    ) -> Result<Self, InferenceError> {
        let context =
            TensorRtInferenceContext::new_auto(device_id).map_err(|e| execution_error(&e))?;
        Ok(Self::from_context(
            context,
            device_id,
            asset_id.into(),
            asset_sha256.into(),
            None,
        ))
    }

    /// Like [`Self::try_new_hardware`], but degrades to a mock Blackwell context when the NVIDIA
    /// stack is unavailable. [`Self::is_hardware_backed`] and [`Self::fallback_reason`] tell
    /// which one you got.
    pub fn new(
        asset_id: impl Into<String>,
        asset_sha256: impl Into<String>,
        device_id: u32,
    ) -> Self {
        let asset_id = asset_id.into();
        let asset_sha256 = asset_sha256.into();
        match TensorRtInferenceContext::new_auto(device_id) {
            Ok(context) => Self::from_context(context, device_id, asset_id, asset_sha256, None),
            Err(error) => Self::from_context(
                TensorRtInferenceContext::new_mock_sm120_fp8(),
                device_id,
                asset_id,
                asset_sha256,
                Some(error.to_string()),
            ),
        }
    }

    /// Loads the compiled stateful TensorRT engines from `model_dir` and wires real GPU inference
    /// through the shared DeepFilterNet3 DSP pipeline.
    pub fn load_stateful(
        model_dir: impl AsRef<Path>,
        device_id: u32,
        asset_id: impl Into<String>,
        asset_sha256: impl Into<String>,
    ) -> Result<Self, InferenceError> {
        let session = TensorRtDfn3Session::load(model_dir.as_ref(), device_id)
            .map_err(|e| execution_error(&e))?;

        let asset_id = asset_id.into();
        let asset_sha256 = asset_sha256.into();
        let sm = session.device().sm_string();
        let device_name = session.device().name().to_owned();
        let precision = session.precision();
        let native_version = TensorRtLibrary::load()
            .ok()
            .map(|lib| lib.version_string().to_owned());

        let mut descriptor = Self::descriptor_for_sm(&sm, asset_id, asset_sha256);
        descriptor.runtime_version = "stateful";

        let model = TensorRtSpectralModel {
            session,
            descriptor: descriptor.clone(),
            scratch: Box::new(Dfn3Output::new()),
        };
        let pipeline = AgnosticDspBackend::new(model, DspPipelineConfig::default())?;

        Ok(Self {
            descriptor,
            device_id,
            device_name,
            sm_version: sm,
            precision,
            engine: Engine::Neural(Box::new(pipeline)),
            fallback_reason: None,
            simulated_failure: false,
            native_version,
        })
    }

    /// Automatically loads the stateful TensorRT engines on GPU device 0.
    pub fn load_stateful_auto(
        model_dir: impl AsRef<Path>,
        asset_id: impl Into<String>,
        asset_sha256: impl Into<String>,
    ) -> Result<Self, InferenceError> {
        Self::load_stateful(model_dir, 0, asset_id, asset_sha256)
    }

    #[must_use]
    pub fn from_manifest(manifest: &ApprovedAssetManifest) -> Self {
        Self::new(manifest.asset_id(), manifest.asset_sha256(), 0)
    }

    /// Mock backend on an emulated Blackwell `sm_120` (FP8); never touches the host GPU.
    #[must_use]
    pub fn new_mock() -> Self {
        Self::from_context(
            TensorRtInferenceContext::new_mock_sm120_fp8(),
            0,
            DEFAULT_ASSET_ID.to_owned(),
            MOCK_ASSET_SHA256.to_owned(),
            None,
        )
    }

    /// Mock backend on an emulated GPU, with the precision the policy picks for it.
    pub fn new_mock_for_device(device: CudaDevice) -> Result<Self, InferenceError> {
        let name = device.name().to_owned();
        let context = TensorRtInferenceContext::new_mock_preferred(device).ok_or_else(|| {
            InferenceError::InferenceExecution(format!(
                "{name} supports none of FP8/FP16/INT8; another backend must be used"
            ))
        })?;
        Ok(Self::from_context(
            context,
            0,
            DEFAULT_ASSET_ID.to_owned(),
            MOCK_ASSET_SHA256.to_owned(),
            None,
        ))
    }

    /// Whether the CUDA driver and `libnvinfer` can both be loaded on this host.
    #[must_use]
    pub fn is_available() -> bool {
        TensorRtRuntime::is_available()
    }

    #[must_use]
    pub const fn device_id(&self) -> u32 {
        self.device_id
    }

    /// Precision selected for the GPU by the Clearcore policy.
    #[must_use]
    pub const fn precision(&self) -> PrecisionTarget {
        self.precision
    }

    /// Name of the GPU (or of the emulated GPU for mock backends).
    #[must_use]
    pub fn device_name(&self) -> &str {
        &self.device_name
    }

    /// Compute capability string such as `sm_120`.
    #[must_use]
    pub fn sm_version(&self) -> String {
        self.sm_version.clone()
    }

    /// True when a real CUDA context, stream and VRAM buffers back this backend.
    #[must_use]
    pub const fn is_hardware_backed(&self) -> bool {
        match &self.engine {
            Engine::Passthrough(ctx) => ctx.is_hardware_backed(),
            Engine::Neural(_) => true,
        }
    }

    /// Whether frames run through a deserialized TensorRT engine.
    #[must_use]
    pub const fn executes_engine(&self) -> bool {
        matches!(self.engine, Engine::Neural(_))
    }

    /// Whether frames go through a neural network.
    #[must_use]
    pub const fn executes_inference(&self) -> bool {
        self.executes_engine()
    }

    /// Alias of [`Self::executes_inference`].
    #[must_use]
    pub const fn is_hardware_accelerated(&self) -> bool {
        self.executes_inference()
    }

    /// Why [`Self::new`] had to use a mock context, if it did.
    #[must_use]
    pub fn fallback_reason(&self) -> Option<&str> {
        self.fallback_reason.as_deref()
    }

    /// Version of the loaded `libnvinfer`, or `None` for mock backends.
    #[must_use]
    pub fn native_runtime_version(&self) -> Option<&str> {
        self.native_version.as_deref()
    }

    pub const fn set_simulated_failure(&mut self, fail: bool) {
        self.simulated_failure = fail;
        if let Engine::Passthrough(ref mut ctx) = self.engine {
            ctx.set_simulated_failure(fail);
        }
    }

    /// Resets the recurrent state of the network and DSP pipeline.
    pub fn reset(&mut self) -> Result<(), InferenceError> {
        match &mut self.engine {
            Engine::Passthrough(_) => Ok(()),
            Engine::Neural(pipeline) => pipeline.reset(),
        }
    }
}

impl InferenceBackend for TensorRtBackend {
    fn descriptor(&self) -> BackendDescriptor {
        self.descriptor.clone()
    }

    fn process(&mut self, input: &AudioFrame) -> Result<ProcessedFrame, InferenceError> {
        if self.simulated_failure {
            return Err(InferenceError::InferenceExecution(
                "TensorRT engine execution failed".to_owned(),
            ));
        }

        if input.iter().any(|sample| !sample.is_finite()) {
            return Err(InferenceError::InputContract(
                "input frame contains a non-finite sample".to_owned(),
            ));
        }

        match &mut self.engine {
            Engine::Passthrough(ctx) => {
                let mut output = [0.0f32; HOP_SAMPLES];
                ctx.process_frame(input, &mut output)
                    .map_err(|error| execution_error(&error))?;
                ProcessedFrame::checked(output, ALGORITHM_LATENCY_SAMPLES, self.descriptor())
            }
            Engine::Neural(pipeline) => match pipeline.process(input) {
                Ok(frame) => Ok(frame),
                Err(error) => {
                    let _ = pipeline.reset();
                    Err(error)
                }
            },
        }
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
