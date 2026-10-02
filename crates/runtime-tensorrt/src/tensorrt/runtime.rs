use std::sync::Arc;

use crate::cuda::bindings::{CudaDriver, is_cuda_driver_available};
use crate::cuda::device::{GpuDevice, enumerate_devices};
use crate::error::TensorRtError;
use crate::precision::PrecisionTarget;
use crate::tensorrt::bindings::{TensorRtLibrary, is_tensorrt_available};
use crate::tensorrt::context::TensorRtInferenceContext;

/// Safe interface to the NVIDIA TensorRT runtime on the host system.
#[derive(Debug)]
pub struct TensorRtRuntime {
    library: Arc<TensorRtLibrary>,
    cuda_driver: Arc<CudaDriver>,
    devices: Vec<GpuDevice>,
}

impl TensorRtRuntime {
    /// Attempts to initialize the TensorRT runtime, verifying both CUDA Driver and TensorRT library.
    pub fn new() -> Result<Self, TensorRtError> {
        let cuda_driver = CudaDriver::load().map_err(TensorRtError::Cuda)?;
        let library = TensorRtLibrary::load()?;
        let devices = enumerate_devices(&cuda_driver).map_err(TensorRtError::Cuda)?;

        Ok(Self {
            library,
            cuda_driver,
            devices,
        })
    }

    /// Checks whether both the CUDA driver and TensorRT runtime libraries are present on the host.
    #[must_use]
    pub fn is_available() -> bool {
        is_cuda_driver_available() && is_tensorrt_available()
    }

    #[must_use]
    pub fn version(&self) -> &str {
        self.library.version_string()
    }

    #[must_use]
    pub fn devices(&self) -> &[GpuDevice] {
        &self.devices
    }

    #[must_use]
    pub fn cuda_driver(&self) -> &Arc<CudaDriver> {
        &self.cuda_driver
    }

    /// Finds a Blackwell sm_120 GPU if present among detected devices.
    #[must_use]
    pub fn find_blackwell_device(&self) -> Option<&GpuDevice> {
        self.devices.iter().find(|d| d.is_blackwell())
    }

    /// Creates an inference context on the chosen device with precision targeting.
    pub fn create_context(
        &self,
        device_ordinal: u32,
        precision: PrecisionTarget,
    ) -> Result<TensorRtInferenceContext, TensorRtError> {
        TensorRtInferenceContext::new(device_ordinal, precision)
    }
}
