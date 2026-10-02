use std::ffi::{c_char, c_int};
use std::sync::Arc;

use crate::cuda::bindings::{
    CU_DEVICE_ATTRIBUTE_COMPUTE_CAPABILITY_MAJOR, CU_DEVICE_ATTRIBUTE_COMPUTE_CAPABILITY_MINOR,
    CU_DEVICE_ATTRIBUTE_MULTIPROCESSOR_COUNT, CUDA_SUCCESS, CudaDriver,
};
use crate::error::CudaError;
use crate::precision::PrecisionTarget;

/// Information about a detected physical NVIDIA GPU device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GpuDevice {
    ordinal: u32,
    name: String,
    compute_capability: (i32, i32),
    multiprocessor_count: u32,
}

impl GpuDevice {
    /// Queries the CUDA driver for the device at the given ordinal.
    pub fn query(driver: &Arc<CudaDriver>, ordinal: u32) -> Result<Self, CudaError> {
        let funcs = driver.functions();
        let mut dev: c_int = 0;

        // SAFETY: cu_device_get populates dev for a valid ordinal.
        let res = unsafe { (funcs.cu_device_get)(&raw mut dev, ordinal as c_int) };
        if res != CUDA_SUCCESS {
            return Err(CudaError::DeviceNotFound(ordinal));
        }

        // Query device name
        let mut name_buf = [0 as c_char; 256];
        // SAFETY: Buffer size is 256 and name_buf is valid.
        let name_res = unsafe {
            (funcs.cu_device_get_name)(name_buf.as_mut_ptr(), name_buf.len() as c_int, dev)
        };
        let name = if name_res == CUDA_SUCCESS {
            // SAFETY: name_buf contains a null-terminated string populated by cuDeviceGetName.
            unsafe { std::ffi::CStr::from_ptr(name_buf.as_ptr()) }
                .to_string_lossy()
                .trim()
                .to_owned()
        } else {
            format!("NVIDIA GPU {ordinal}")
        };

        // Query compute capability
        let mut major: c_int = 0;
        let mut minor: c_int = 0;
        // SAFETY: Querying standard compute capability attributes on valid device.
        let maj_res = unsafe {
            (funcs.cu_device_get_attribute)(
                &raw mut major,
                CU_DEVICE_ATTRIBUTE_COMPUTE_CAPABILITY_MAJOR,
                dev,
            )
        };
        // SAFETY: Querying standard compute capability attributes on valid device.
        let min_res = unsafe {
            (funcs.cu_device_get_attribute)(
                &raw mut minor,
                CU_DEVICE_ATTRIBUTE_COMPUTE_CAPABILITY_MINOR,
                dev,
            )
        };

        if maj_res != CUDA_SUCCESS || min_res != CUDA_SUCCESS {
            return Err(CudaError::DriverCallFailed {
                function: "cuDeviceGetAttribute(COMPUTE_CAPABILITY)",
                code: if maj_res != CUDA_SUCCESS {
                    maj_res
                } else {
                    min_res
                },
            });
        }

        // Query multiprocessor count
        let mut mp_count: c_int = 0;
        // SAFETY: Querying multiprocessor count attribute on valid device.
        let mp_res = unsafe {
            (funcs.cu_device_get_attribute)(
                &raw mut mp_count,
                CU_DEVICE_ATTRIBUTE_MULTIPROCESSOR_COUNT,
                dev,
            )
        };
        let multiprocessor_count = if mp_res == CUDA_SUCCESS && mp_count > 0 {
            mp_count as u32
        } else {
            1
        };

        Ok(Self {
            ordinal,
            name,
            compute_capability: (major, minor),
            multiprocessor_count,
        })
    }

    /// Creates a mock GPU device with an arbitrary compute capability (offline policy tests).
    #[must_use]
    pub fn mock(name: impl Into<String>, compute_capability: (i32, i32)) -> Self {
        Self {
            ordinal: 0,
            name: name.into(),
            compute_capability,
            multiprocessor_count: 1,
        }
    }

    /// Creates a mock GPU device representing Blackwell sm_120 for tests.
    #[must_use]
    pub fn mock_blackwell_sm120() -> Self {
        Self {
            ordinal: 0,
            name: "NVIDIA RTX PRO 1000 Blackwell Generation Laptop GPU".to_owned(),
            compute_capability: (12, 0),
            multiprocessor_count: 24,
        }
    }

    #[must_use]
    pub const fn ordinal(&self) -> u32 {
        self.ordinal
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub const fn compute_capability(&self) -> (i32, i32) {
        self.compute_capability
    }

    #[must_use]
    pub const fn multiprocessor_count(&self) -> u32 {
        self.multiprocessor_count
    }

    /// Formats the SM compute capability as a string (e.g. "sm_120", "sm_89", "sm_80").
    #[must_use]
    pub fn sm_string(&self) -> String {
        format!(
            "sm_{}{}",
            self.compute_capability.0, self.compute_capability.1
        )
    }

    /// Returns true if this device is NVIDIA Blackwell architecture (`sm_120`, Compute Capability 12.0).
    #[must_use]
    pub const fn is_blackwell(&self) -> bool {
        self.compute_capability.0 == 12 && self.compute_capability.1 == 0
    }

    /// Alias for `is_blackwell`.
    #[must_use]
    pub const fn is_sm120(&self) -> bool {
        self.is_blackwell()
    }

    /// Returns the precision targets supported on this GPU device, best first.
    ///
    /// - FP8 tensor cores: Ada Lovelace `sm_89`, Hopper `sm_90` and every later architecture
    ///   (Blackwell `sm_100` / `sm_120`).
    /// - FP16: Volta `sm_70` and later.
    /// - INT8 (`DP4A`): Pascal `sm_61` and later.
    /// - FP32 is always listed as a reference target, but it is never the preferred one.
    #[must_use]
    pub fn supported_precisions(&self) -> Vec<PrecisionTarget> {
        let cc = self.compute_capability;
        let mut precisions = Vec::with_capacity(4);
        if cc >= (8, 9) {
            precisions.push(PrecisionTarget::Fp8);
        }
        if cc.0 >= 7 {
            precisions.push(PrecisionTarget::Fp16);
        }
        if cc >= (6, 1) {
            precisions.push(PrecisionTarget::Int8);
        }
        precisions.push(PrecisionTarget::Fp32);
        precisions
    }

    /// Clearcore precision policy for this GPU: FP8 where the hardware supports it,
    /// otherwise FP16, otherwise INT8. FP32 is never chosen for a GPU, so a device without
    /// any of those returns `None` and the caller must fall back to another backend.
    #[must_use]
    pub fn preferred_precision(&self) -> Option<PrecisionTarget> {
        self.supported_precisions()
            .into_iter()
            .find(|p| *p != PrecisionTarget::Fp32)
    }

    /// Checks if a specific precision target is supported on this GPU.
    #[must_use]
    pub fn supports_precision(&self, precision: PrecisionTarget) -> bool {
        self.supported_precisions().contains(&precision)
    }
}

/// Enumerate all detected NVIDIA GPU devices via the CUDA driver.
pub fn enumerate_devices(driver: &Arc<CudaDriver>) -> Result<Vec<GpuDevice>, CudaError> {
    let funcs = driver.functions();
    let mut count: c_int = 0;
    // SAFETY: cu_device_get_count writes to count.
    let res = unsafe { (funcs.cu_device_get_count)(&raw mut count) };
    if res != CUDA_SUCCESS {
        return Err(CudaError::DriverCallFailed {
            function: "cuDeviceGetCount",
            code: res,
        });
    }

    let mut devices = Vec::with_capacity(count.max(0) as usize);
    for ordinal in 0..count.max(0) as u32 {
        let dev = GpuDevice::query(driver, ordinal)?;
        devices.push(dev);
    }
    Ok(devices)
}
