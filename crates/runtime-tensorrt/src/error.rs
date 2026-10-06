use std::fmt;

use crate::precision::PrecisionTarget;

/// Errors arising from CUDA Driver API interactions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CudaError {
    /// CUDA driver library (libcuda.so.1 / nvcuda.dll) was not found.
    DriverNotFound(String),
    /// A CUDA driver function symbol was missing from the loaded library.
    SymbolNotFound(&'static str),
    /// A CUDA driver API call returned a non-zero error code.
    DriverCallFailed { function: &'static str, code: i32 },
    /// Requested GPU device index was out of range or not found.
    DeviceNotFound(u32),
    /// Device memory allocation failed.
    MemoryAllocationFailed { bytes: usize, code: i32 },
    /// Asynchronous host-device memory copy failed.
    CopyFailed { direction: &'static str, code: i32 },
    /// CUDA stream operation failed.
    StreamError { action: &'static str, code: i32 },
    /// Stream synchronization timed out waiting for the GPU to complete.
    SyncTimeout,
}

impl fmt::Display for CudaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DriverNotFound(msg) => write!(f, "CUDA driver library not found: {msg}"),
            Self::SymbolNotFound(sym) => write!(f, "CUDA driver symbol '{sym}' not found"),
            Self::DriverCallFailed { function, code } => {
                write!(
                    f,
                    "CUDA driver call '{function}' failed with error code {code}"
                )
            }
            Self::DeviceNotFound(id) => write!(f, "NVIDIA GPU device {id} not found"),
            Self::MemoryAllocationFailed { bytes, code } => {
                write!(
                    f,
                    "Failed to allocate {bytes} bytes of GPU VRAM (code {code})"
                )
            }
            Self::CopyFailed { direction, code } => {
                write!(
                    f,
                    "CUDA memory copy ({direction}) failed with error code {code}"
                )
            }
            Self::StreamError { action, code } => {
                write!(f, "CUDA stream '{action}' failed with error code {code}")
            }
            Self::SyncTimeout => write!(f, "CUDA stream synchronization timed out"),
        }
    }
}

impl std::error::Error for CudaError {}

/// Errors arising from TensorRT runtime and inference context execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TensorRtError {
    /// TensorRT library (libnvinfer.so / nvinfer.dll) was not found.
    LibraryNotFound(String),
    /// TensorRT runtime symbol was missing from the loaded library.
    SymbolNotFound(&'static str),
    /// Initialization of the TensorRT inference context failed.
    InitializationFailed(String),
    /// TensorRT inference execution failed.
    ExecutionFailed(String),
    /// Output buffer contained non-finite values (NaN / Inf) violating the fail-closed contract.
    NonFiniteOutputDetected,
    /// Requested precision target is not supported on the target GPU architecture.
    UnsupportedPrecision {
        requested: PrecisionTarget,
        device_name: String,
        sm_version: String,
    },
    /// The GPU supports none of the allowed GPU precisions (FP8, FP16, INT8).
    NoSupportedPrecision {
        device_name: String,
        sm_version: String,
    },
    /// Loading or deserializing the TensorRT model plan failed.
    ModelLoadFailed(String),
    /// Underlying CUDA driver error.
    Cuda(CudaError),
}

impl fmt::Display for TensorRtError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LibraryNotFound(msg) => write!(f, "TensorRT library not found: {msg}"),
            Self::SymbolNotFound(sym) => write!(f, "TensorRT symbol '{sym}' not found"),
            Self::InitializationFailed(msg) => {
                write!(f, "TensorRT initialization failed: {msg}")
            }
            Self::ExecutionFailed(msg) => write!(f, "TensorRT execution failed: {msg}"),
            Self::NonFiniteOutputDetected => {
                write!(
                    f,
                    "TensorRT produced non-finite output (NaN/Inf) - fail-closed silence applied"
                )
            }
            Self::UnsupportedPrecision {
                requested,
                device_name,
                sm_version,
            } => {
                write!(
                    f,
                    "Precision target {requested:?} is unsupported on {device_name} ({sm_version})"
                )
            }
            Self::NoSupportedPrecision {
                device_name,
                sm_version,
            } => write!(
                f,
                "{device_name} ({sm_version}) supports none of FP8/FP16/INT8; falling back to another backend"
            ),
            Self::ModelLoadFailed(msg) => write!(f, "TensorRT model load failed: {msg}"),
            Self::Cuda(err) => write!(f, "CUDA error: {err}"),
        }
    }
}

impl std::error::Error for TensorRtError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Cuda(err) => Some(err),
            _ => None,
        }
    }
}

impl From<CudaError> for TensorRtError {
    fn from(err: CudaError) -> Self {
        Self::Cuda(err)
    }
}
