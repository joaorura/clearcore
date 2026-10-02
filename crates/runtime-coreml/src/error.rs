use std::fmt;
use std::path::PathBuf;

use realtime_noise_model::InferenceError;

/// Errors emitted during CoreML model initialization, compilation, loading, or execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreMlError {
    /// The specified `.mlmodelc` or `.mlmodel` file was not found.
    ModelNotFound(PathBuf),
    /// Failure compiling or loading the model bundle into CoreML.
    ModelLoadFailed(String),
    /// Failure configuring CoreML compute units or options.
    ConfigurationFailed(String),
    /// Failure allocating or binding an `MLMultiArray` I/O tensor.
    TensorAllocationFailed(String),
    /// Failure during inference prediction step.
    InferenceExecution(String),
    /// Invalid tensor shape supplied for inference.
    InvalidShape {
        expected: Vec<usize>,
        actual: Vec<usize>,
    },
    /// The current platform or hardware architecture does not support CoreML acceleration.
    PlatformNotSupported(String),
    /// Input or output data contract violation (e.g., non-finite samples or incorrect hop length).
    ContractViolation(String),
    /// Execution failed due to a simulated failure mode.
    SimulatedFailure,
}

impl fmt::Display for CoreMlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ModelNotFound(path) => {
                write!(f, "CoreML model bundle not found: {}", path.display())
            }
            Self::ModelLoadFailed(msg) => write!(f, "failed to load CoreML model: {msg}"),
            Self::ConfigurationFailed(msg) => {
                write!(f, "failed to configure CoreML runtime: {msg}")
            }
            Self::TensorAllocationFailed(msg) => {
                write!(f, "failed to allocate CoreML MLMultiArray tensor: {msg}")
            }
            Self::InferenceExecution(msg) => {
                write!(f, "CoreML inference prediction failed: {msg}")
            }
            Self::InvalidShape { expected, actual } => {
                write!(
                    f,
                    "invalid CoreML tensor shape: expected {expected:?}, got {actual:?}"
                )
            }
            Self::PlatformNotSupported(msg) => {
                write!(f, "CoreML platform unsupported: {msg}")
            }
            Self::ContractViolation(msg) => {
                write!(f, "CoreML audio contract violation: {msg}")
            }
            Self::SimulatedFailure => {
                write!(f, "CoreML simulated failure triggered")
            }
        }
    }
}

impl std::error::Error for CoreMlError {}

impl From<CoreMlError> for InferenceError {
    fn from(err: CoreMlError) -> Self {
        match err {
            CoreMlError::ModelNotFound(p) => {
                Self::ModelCorruption(format!("CoreML model not found: {}", p.display()))
            }
            CoreMlError::ModelLoadFailed(msg) => Self::ModelCorruption(msg),
            CoreMlError::ConfigurationFailed(msg) => Self::InferenceExecution(msg),
            CoreMlError::TensorAllocationFailed(msg) => Self::InferenceExecution(msg),
            CoreMlError::InferenceExecution(msg) => Self::InferenceExecution(msg),
            CoreMlError::InvalidShape { expected, actual } => Self::InputContract(format!(
                "invalid shape: expected {expected:?}, got {actual:?}"
            )),
            CoreMlError::PlatformNotSupported(msg) => Self::UnsupportedCpuProfile(msg),
            CoreMlError::ContractViolation(msg) => Self::InputContract(msg),
            CoreMlError::SimulatedFailure => {
                Self::InferenceExecution("CoreML simulated failure".to_owned())
            }
        }
    }
}
