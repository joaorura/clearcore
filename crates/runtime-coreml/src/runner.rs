use std::path::Path;

use realtime_noise_contracts::AudioFrame;

use crate::error::CoreMlError;
use crate::ffi::platform::CoreMlEngineHandle;
use crate::tensor::CoreMlTensor;
use crate::types::{ComputeUnit, is_apple_silicon_available};

/// High-level model runner for compiled `.mlmodelc` bundles.
///
/// Features pre-allocated I/O tensors to ensure zero heap allocations
/// during the 10ms (480-sample) audio frame processing cycle.
#[derive(Debug)]
pub struct CoreMlModelRunner {
    engine: CoreMlEngineHandle,
    input_tensor: CoreMlTensor,
    output_tensor: CoreMlTensor,
    compute_unit: ComputeUnit,
    model_name: String,
    simulated_failure: bool,
}

impl CoreMlModelRunner {
    /// Loads a compiled CoreML model bundle from disk.
    pub fn load(
        model_path: impl AsRef<Path>,
        compute_unit: ComputeUnit,
    ) -> Result<Self, CoreMlError> {
        let path = model_path.as_ref();
        let engine = CoreMlEngineHandle::load(path, compute_unit)?;
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("coreml-model")
            .to_owned();

        Ok(Self {
            engine,
            input_tensor: CoreMlTensor::for_audio_frame(),
            output_tensor: CoreMlTensor::for_audio_frame(),
            compute_unit,
            model_name: name,
            simulated_failure: false,
        })
    }

    /// Creates a simulated runner instance for mock environments, unit tests, and cross-platform testing.
    #[must_use]
    pub fn new_simulated(model_name: &str, compute_unit: ComputeUnit) -> Self {
        let engine = CoreMlEngineHandle::new_simulated(model_name, compute_unit);

        Self {
            engine,
            input_tensor: CoreMlTensor::for_audio_frame(),
            output_tensor: CoreMlTensor::for_audio_frame(),
            compute_unit,
            model_name: model_name.to_owned(),
            simulated_failure: false,
        }
    }

    /// Evaluates one standard [`AudioFrame`] (480 samples @ 48 kHz mono).
    ///
    /// This method performs zero heap allocations on the critical audio thread path.
    pub fn run_frame(&mut self, input: &AudioFrame) -> Result<AudioFrame, CoreMlError> {
        if self.simulated_failure {
            return Err(CoreMlError::SimulatedFailure);
        }

        self.input_tensor.copy_from_audio_frame(input)?;
        self.engine
            .predict(&self.input_tensor, &mut self.output_tensor)?;
        self.output_tensor.copy_to_audio_frame()
    }

    /// Evaluates raw slices of float32 samples.
    pub fn run_slice(&mut self, input: &[f32], output: &mut [f32]) -> Result<(), CoreMlError> {
        if self.simulated_failure {
            return Err(CoreMlError::SimulatedFailure);
        }

        self.input_tensor.copy_from_slice(input)?;
        self.engine
            .predict(&self.input_tensor, &mut self.output_tensor)?;
        self.output_tensor.copy_to_slice(output)
    }

    /// Configured compute unit (ANE, GPU Metal, or CPU).
    #[must_use]
    pub const fn compute_unit(&self) -> ComputeUnit {
        self.compute_unit
    }

    /// Associated model identifier.
    #[must_use]
    pub fn model_name(&self) -> &str {
        &self.model_name
    }

    /// Whether `run_frame` evaluates a loaded CoreML model. `false` for simulated runners and on
    /// platforms without CoreML, where frames are copied through unchanged.
    #[must_use]
    pub fn executes_inference(&self) -> bool {
        self.engine.executes_inference()
    }

    /// Whether this runner evaluates a model on Apple Silicon hardware acceleration: a real model
    /// is loaded, the host is Apple Silicon and the compute unit is not CPU-only.
    #[must_use]
    pub fn is_hardware_accelerated(&self) -> bool {
        self.executes_inference()
            && is_apple_silicon_available()
            && self.compute_unit != ComputeUnit::CpuOnly
    }

    /// Configures whether simulated failure is enabled.
    pub fn set_simulated_failure(&mut self, fail: bool) {
        self.simulated_failure = fail;
        self.engine.set_simulated_failure(fail);
    }

    /// Access to the internal input tensor buffer.
    #[must_use]
    pub const fn input_tensor(&self) -> &CoreMlTensor {
        &self.input_tensor
    }

    /// Access to the internal output tensor buffer.
    #[must_use]
    pub const fn output_tensor(&self) -> &CoreMlTensor {
        &self.output_tensor
    }
}

impl Clone for CoreMlModelRunner {
    fn clone(&self) -> Self {
        Self {
            engine: self.engine.clone(),
            input_tensor: self.input_tensor.clone(),
            output_tensor: self.output_tensor.clone(),
            compute_unit: self.compute_unit,
            model_name: self.model_name.clone(),
            simulated_failure: self.simulated_failure,
        }
    }
}
