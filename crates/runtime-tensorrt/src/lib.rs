//! TensorRT and CUDA runtime abstractions for Clearcore.

pub mod cuda;
pub mod error;
pub mod precision;
pub mod tensorrt;

pub use cuda::{
    CudaContext, CudaDevice, CudaStream, DeviceCopy, GpuBuffer, is_cuda_driver_available,
};
pub use error::{CudaError, TensorRtError};
pub use precision::PrecisionTarget;
pub use tensorrt::dfn3;
pub use tensorrt::{
    Dfn3Engines, Dfn3Output, TensorRtContext, TensorRtDfn3Session, TensorRtEngine,
    TensorRtExecutionContext, TensorRtInferenceContext, TensorRtLibrary, TensorRtRuntime,
    is_tensorrt_available, locate_library as locate_tensorrt_library,
};
