use std::sync::Arc;

use crate::cuda::bindings::{CUDA_ERROR_NOT_READY, CUDA_SUCCESS, CUstream, CudaDriver};
use crate::error::CudaError;

/// Safe RAII wrapper around a CUDA Stream (`CUstream`).
///
/// Ensures `cuStreamDestroy` is cleanly called on Drop to prevent GPU stream leaks.
#[derive(Debug)]
pub struct CudaStream {
    driver: Arc<CudaDriver>,
    stream: CUstream,
}

// SAFETY: CUstream is safe to transfer between threads.
unsafe impl Send for CudaStream {}
// SAFETY: CUstream synchronization is handled via CUDA Driver API.
unsafe impl Sync for CudaStream {}

impl CudaStream {
    /// Creates a new CUDA stream (non-blocking / priority where available).
    pub fn new(driver: Arc<CudaDriver>) -> Result<Self, CudaError> {
        let funcs = driver.functions();
        let mut stream: CUstream = std::ptr::null_mut();

        // Flags: 1 = CU_STREAM_NON_BLOCKING (does not synchronize with default stream)
        // SAFETY: Allocating a new stream handle.
        let res = unsafe { (funcs.cu_stream_create)(&raw mut stream, 1) };
        if res != CUDA_SUCCESS || stream.is_null() {
            return Err(CudaError::StreamError {
                action: "cuStreamCreate",
                code: res,
            });
        }

        Ok(Self { driver, stream })
    }

    /// Blocks the host CPU until all tasks enqueued on this stream have finished on the GPU.
    pub fn synchronize(&self) -> Result<(), CudaError> {
        let funcs = self.driver.functions();
        // SAFETY: stream is a valid handle.
        let res = unsafe { (funcs.cu_stream_synchronize)(self.stream) };
        if res != CUDA_SUCCESS {
            return Err(CudaError::StreamError {
                action: "cuStreamSynchronize",
                code: res,
            });
        }
        Ok(())
    }

    /// Queries the stream to determine if all enqueued operations have completed.
    /// Returns `Ok(true)` if done, `Ok(false)` if still working, or Err on error.
    pub fn is_complete(&self) -> Result<bool, CudaError> {
        let funcs = self.driver.functions();
        // SAFETY: stream is a valid handle.
        let res = unsafe { (funcs.cu_stream_query)(self.stream) };
        if res == CUDA_SUCCESS {
            Ok(true)
        } else if res == CUDA_ERROR_NOT_READY {
            Ok(false)
        } else {
            Err(CudaError::StreamError {
                action: "cuStreamQuery",
                code: res,
            })
        }
    }

    #[must_use]
    pub const fn raw_stream(&self) -> CUstream {
        self.stream
    }
}

impl Drop for CudaStream {
    fn drop(&mut self) {
        if !self.stream.is_null() {
            let funcs = self.driver.functions();
            // SAFETY: Destroying stream managed solely by this RAII handle.
            unsafe {
                (funcs.cu_stream_destroy)(self.stream);
            }
            self.stream = std::ptr::null_mut();
        }
    }
}
