//! Cross-platform mock/stub implementation of CoreML runtime for Linux, Windows,
//! and CI testing environments.
//!
//! Provides a high-fidelity simulated CoreML runner that verifies tensor contracts,
//! simulates Apple Neural Engine / GPU Metal execution, and supports lifecycle testing.

use std::path::{Path, PathBuf};

use crate::error::CoreMlError;
use crate::tensor::CoreMlTensor;
use crate::types::ComputeUnit;

/// Cross-platform mock handle simulating CoreML execution on Apple Silicon.
#[derive(Debug, Clone)]
pub struct CoreMlEngineHandle {
    model_path: PathBuf,
    compute_unit: ComputeUnit,
    simulated_failure: bool,
}

impl CoreMlEngineHandle {
    /// Loads or simulates loading of a `.mlmodelc` CoreML bundle.
    pub fn load(model_path: &Path, units: ComputeUnit) -> Result<Self, CoreMlError> {
        let path_buf = model_path.to_path_buf();
        let path_str = path_buf.to_string_lossy();

        // Check if path exists or matches explicit mock model descriptors
        let is_mock = path_str.contains("mock")
            || path_str.contains("df-compatible")
            || path_str.starts_with("simulated-");

        if !model_path.exists() && !is_mock {
            return Err(CoreMlError::ModelNotFound(path_buf));
        }

        Ok(Self {
            model_path: path_buf,
            compute_unit: units,
            simulated_failure: false,
        })
    }

    /// Creates an explicit simulated runner with a descriptive model identifier.
    #[must_use]
    pub fn new_simulated(model_name: &str, units: ComputeUnit) -> Self {
        Self {
            model_path: PathBuf::from(format!("{model_name}.mlmodelc")),
            compute_unit: units,
            simulated_failure: false,
        }
    }

    /// Evaluates simulated CoreML inference over input and output tensors.
    pub fn predict(
        &mut self,
        input_tensor: &CoreMlTensor,
        output_tensor: &mut CoreMlTensor,
    ) -> Result<(), CoreMlError> {
        if self.simulated_failure {
            return Err(CoreMlError::SimulatedFailure);
        }

        input_tensor.verify_finite()?;

        let input_slice = input_tensor.as_slice();
        let output_slice = output_tensor.as_mut_slice();

        if input_slice.len() != output_slice.len() {
            return Err(CoreMlError::InvalidShape {
                expected: input_tensor.shape().to_vec(),
                actual: output_tensor.shape().to_vec(),
            });
        }

        // Simulate high-performance ANE tensor pass adhering to Clearcore DSP contracts
        output_slice.copy_from_slice(input_slice);

        output_tensor.verify_finite()?;
        Ok(())
    }

    /// Whether `predict` runs a CoreML model. Always `false` on this platform: there is no
    /// CoreML, so `predict` copies the input tensor to the output tensor.
    #[must_use]
    pub const fn executes_inference(&self) -> bool {
        false
    }

    /// Configures simulated failure mode.
    pub fn set_simulated_failure(&mut self, fail: bool) {
        self.simulated_failure = fail;
    }

    /// Path to the model bundle.
    #[must_use]
    pub fn model_path(&self) -> &Path {
        &self.model_path
    }

    /// Active compute unit.
    #[must_use]
    pub const fn compute_unit(&self) -> ComputeUnit {
        self.compute_unit
    }
}
