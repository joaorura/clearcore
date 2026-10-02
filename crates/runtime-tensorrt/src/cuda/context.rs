use std::ffi::c_int;
use std::sync::Arc;

use crate::cuda::bindings::{CUDA_SUCCESS, CUcontext, CUdevice, CudaDriver};
use crate::error::CudaError;

/// Safe RAII wrapper around a CUDA execution context (`CUcontext`).
///
/// Ensures `cuCtxDestroy` is cleanly called on Drop to prevent GPU resource leaks.
#[derive(Debug)]
pub struct CudaContext {
    driver: Arc<CudaDriver>,
    ctx: CUcontext,
    device_ordinal: u32,
}

// SAFETY: CUcontext can be sent across threads.
unsafe impl Send for CudaContext {}
// SAFETY: CUcontext synchronization is handled via CUDA Driver API.
unsafe impl Sync for CudaContext {}

impl CudaContext {
    /// Creates a new CUDA context on the specified device ordinal.
    pub fn new(driver: Arc<CudaDriver>, device_ordinal: u32) -> Result<Self, CudaError> {
        let funcs = driver.functions();
        let mut dev: CUdevice = 0;

        // SAFETY: Querying valid device handle.
        let dev_res = unsafe { (funcs.cu_device_get)(&raw mut dev, device_ordinal as c_int) };
        if dev_res != CUDA_SUCCESS {
            return Err(CudaError::DeviceNotFound(device_ordinal));
        }

        let mut ctx: CUcontext = std::ptr::null_mut();
        // SAFETY: cu_ctx_create creates a new primary context. Flags: 0 (SCHED_AUTO).
        let ctx_res = unsafe { (funcs.cu_ctx_create)(&raw mut ctx, 0, dev) };
        if ctx_res != CUDA_SUCCESS || ctx.is_null() {
            return Err(CudaError::DriverCallFailed {
                function: "cuCtxCreate",
                code: ctx_res,
            });
        }

        Ok(Self {
            driver,
            ctx,
            device_ordinal,
        })
    }

    /// Sets this context as the current calling thread's CUDA context.
    pub fn make_current(&self) -> Result<(), CudaError> {
        let funcs = self.driver.functions();
        // SAFETY: ctx is a valid context handle.
        let res = unsafe { (funcs.cu_ctx_set_current)(self.ctx) };
        if res != CUDA_SUCCESS {
            return Err(CudaError::DriverCallFailed {
                function: "cuCtxSetCurrent",
                code: res,
            });
        }
        Ok(())
    }

    #[must_use]
    pub const fn raw_context(&self) -> CUcontext {
        self.ctx
    }

    #[must_use]
    pub const fn device_ordinal(&self) -> u32 {
        self.device_ordinal
    }

    #[must_use]
    pub fn driver(&self) -> &Arc<CudaDriver> {
        &self.driver
    }
}

impl Drop for CudaContext {
    fn drop(&mut self) {
        if !self.ctx.is_null() {
            let funcs = self.driver.functions();
            // SAFETY: Destroying context managed solely by this RAII handle.
            unsafe {
                (funcs.cu_ctx_destroy)(self.ctx);
            }
            self.ctx = std::ptr::null_mut();
        }
    }
}
