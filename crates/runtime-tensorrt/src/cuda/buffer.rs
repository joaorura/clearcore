use std::ffi::c_void;
use std::marker::PhantomData;
use std::sync::Arc;

use crate::cuda::bindings::{CUDA_SUCCESS, CUdeviceptr, CudaDriver};
use crate::cuda::stream::CudaStream;
use crate::error::CudaError;

mod sealed {
    pub trait Sealed {}
}

/// Marker for element types that may live in a [`GpuBuffer`].
///
/// # Safety
///
/// Implementors must be plain-old-data: no padding, no destructor, no pointers or lifetimes, and
/// **every** bit pattern must be a valid value. The trait is sealed, so only the primitive numeric
/// types below implement it.
pub unsafe trait DeviceCopy: Copy + 'static + sealed::Sealed {}

macro_rules! impl_device_copy {
    ($($t:ty),* $(,)?) => {
        $(
            impl sealed::Sealed for $t {}
            // SAFETY: primitive numeric type; every bit pattern is a valid value.
            unsafe impl DeviceCopy for $t {}
        )*
    };
}

impl_device_copy!(f32, f64, u8, u16, u32, u64, i8, i16, i32, i64);

/// Safe RAII GPU memory buffer allocated in device VRAM via the CUDA driver.
///
/// Encapsulates `cuMemAlloc` and guarantees `cuMemFree` on Drop, eliminating memory leaks.
///
/// The element type is restricted to [`DeviceCopy`]: the buffer is filled and read back as raw
/// bytes, so every bit pattern the device (or an uninitialized allocation) can produce must be a
/// valid `T`. `Copy + 'static` alone is not enough (`bool`, `char`, `&'static T`, `NonZero*` and
/// enums have invalid bit patterns), which is why the trait is sealed and implemented only for the
/// primitive integer and float types.
///
/// ```compile_fail,E0277
/// use realtime_noise_runtime_tensorrt::GpuBuffer;
///
/// // `bool` has invalid bit patterns, so it is not a `DeviceCopy` element type.
/// fn needs_device_copy(_: Option<GpuBuffer<bool>>) {}
/// ```
#[derive(Debug)]
pub struct GpuBuffer<T: DeviceCopy> {
    driver: Arc<CudaDriver>,
    device_ptr: CUdeviceptr,
    len: usize,
    size_bytes: usize,
    _marker: PhantomData<T>,
}

// SAFETY: Device pointers can be safely sent to threads operating within the same CUDA context.
unsafe impl<T: DeviceCopy + Send> Send for GpuBuffer<T> {}
// SAFETY: GPU memory access is synchronized via CUDA streams.
unsafe impl<T: DeviceCopy + Sync> Sync for GpuBuffer<T> {}

impl<T: DeviceCopy> GpuBuffer<T> {
    /// Allocates an uninitialized buffer for `len` elements of type `T` on the GPU.
    pub fn allocate(driver: Arc<CudaDriver>, len: usize) -> Result<Self, CudaError> {
        let size_bytes =
            len.checked_mul(std::mem::size_of::<T>())
                .ok_or(CudaError::MemoryAllocationFailed {
                    bytes: usize::MAX,
                    code: -1,
                })?;

        if size_bytes == 0 {
            return Ok(Self {
                driver,
                device_ptr: 0,
                len: 0,
                size_bytes: 0,
                _marker: PhantomData,
            });
        }

        let funcs = driver.functions();
        let mut dptr: CUdeviceptr = 0;

        // SAFETY: Allocating size_bytes on GPU device.
        let res = unsafe { (funcs.cu_mem_alloc)(&raw mut dptr, size_bytes) };
        if res != CUDA_SUCCESS || dptr == 0 {
            return Err(CudaError::MemoryAllocationFailed {
                bytes: size_bytes,
                code: res,
            });
        }

        Ok(Self {
            driver,
            device_ptr: dptr,
            len,
            size_bytes,
            _marker: PhantomData,
        })
    }

    /// Asynchronously copies elements from a host slice to this GPU buffer via the specified CUDA stream.
    ///
    /// # Pinned-memory premise
    ///
    /// The "`src` may be reused as soon as this returns" guarantee holds only while `src` is
    /// ordinary pageable host memory: for that case the driver stages the data into its own buffer
    /// before returning. A `&[T]` *can* in principle be page-locked by other code (for example with
    /// `cuMemHostRegister`), in which case `cuMemcpyHtoDAsync` becomes truly asynchronous and
    /// returns while the DMA is still reading `src`. This crate never registers or pins host
    /// memory itself, and this call is only sound under that premise. A caller that pins the
    /// memory behind `src` must keep it alive and unmodified until `stream` has been synchronized.
    pub fn async_copy_from_host(
        &mut self,
        src: &[T],
        stream: &CudaStream,
    ) -> Result<(), CudaError> {
        if src.len() != self.len {
            return Err(CudaError::CopyFailed {
                direction: "HostToDevice: length mismatch",
                code: -1,
            });
        }

        if self.size_bytes == 0 {
            return Ok(());
        }

        let funcs = self.driver.functions();
        // SAFETY: src is a valid host slice with size_bytes, and device_ptr is a valid allocation.
        let res = unsafe {
            (funcs.cu_memcpy_htod_async)(
                self.device_ptr,
                src.as_ptr().cast::<c_void>(),
                self.size_bytes,
                stream.raw_stream(),
            )
        };

        if res != CUDA_SUCCESS {
            return Err(CudaError::CopyFailed {
                direction: "HostToDevice",
                code: res,
            });
        }

        Ok(())
    }

    /// Copies this GPU buffer into `dst` and blocks until the copy has finished.
    ///
    /// This is the safe way to read the buffer back: the stream is synchronized before `dst` is
    /// handed back to the caller, so the GPU can never write into it after the borrow ends.
    pub fn copy_to_host(&self, dst: &mut [T], stream: &CudaStream) -> Result<(), CudaError> {
        // SAFETY: `dst` stays mutably borrowed for the whole call and the stream is synchronized
        // before returning, so the asynchronous write has completed by the time the borrow ends.
        // On an enqueue error nothing was queued, so there is nothing to wait for.
        unsafe { self.async_copy_to_host(dst, stream) }?;
        stream.synchronize()
    }

    /// Asynchronously copies elements from this GPU buffer to a host slice via the specified CUDA stream.
    ///
    /// Kept as an `unsafe fn` (rather than synchronizing inside) so the caller can overlap the
    /// transfer with other work; use [`Self::copy_to_host`] unless that overlap is needed.
    ///
    /// # Safety
    ///
    /// The device-to-host transfer is only *enqueued* here and may still be running when this
    /// returns, so the borrow checker cannot see the GPU writing into `dst`. The caller must keep
    /// `dst` alive, unmoved and otherwise unaccessed until `stream` has been synchronized (or
    /// [`CudaStream::is_complete`] returned `true`), and must not drop `self` before that point.
    pub unsafe fn async_copy_to_host(
        &self,
        dst: &mut [T],
        stream: &CudaStream,
    ) -> Result<(), CudaError> {
        if dst.len() != self.len {
            return Err(CudaError::CopyFailed {
                direction: "DeviceToHost: length mismatch",
                code: -1,
            });
        }

        if self.size_bytes == 0 {
            return Ok(());
        }

        let funcs = self.driver.functions();
        // SAFETY: dst is a valid host buffer with size_bytes, and device_ptr is a valid allocation.
        let res = unsafe {
            (funcs.cu_memcpy_dtoh_async)(
                dst.as_mut_ptr().cast::<c_void>(),
                self.device_ptr,
                self.size_bytes,
                stream.raw_stream(),
            )
        };

        if res != CUDA_SUCCESS {
            return Err(CudaError::CopyFailed {
                direction: "DeviceToHost",
                code: res,
            });
        }

        Ok(())
    }

    /// Asynchronously copies elements from another GPU buffer to this one on the given stream.
    pub fn async_copy_from_device(
        &mut self,
        src: &Self,
        stream: &CudaStream,
    ) -> Result<(), CudaError> {
        if src.len != self.len {
            return Err(CudaError::CopyFailed {
                direction: "DeviceToDevice: length mismatch",
                code: -1,
            });
        }

        if self.size_bytes == 0 {
            return Ok(());
        }

        let funcs = self.driver.functions();
        // SAFETY: Both device_ptr buffers are allocated with self.size_bytes.
        let res = unsafe {
            (funcs.cu_memcpy_dtod_async)(
                self.device_ptr,
                src.device_ptr,
                self.size_bytes,
                stream.raw_stream(),
            )
        };

        if res != CUDA_SUCCESS {
            return Err(CudaError::CopyFailed {
                direction: "DeviceToDevice",
                code: res,
            });
        }

        Ok(())
    }

    /// Asynchronously zero-fills this GPU buffer on the given stream.
    pub fn async_zero_fill(&mut self, stream: &CudaStream) -> Result<(), CudaError> {
        if self.size_bytes == 0 {
            return Ok(());
        }

        let funcs = self.driver.functions();
        // SAFETY: device_ptr is allocated with self.size_bytes.
        let res = unsafe {
            (funcs.cu_memset_d8_async)(
                self.device_ptr,
                0,
                self.size_bytes,
                stream.raw_stream(),
            )
        };

        if res != CUDA_SUCCESS {
            return Err(CudaError::CopyFailed {
                direction: "MemsetD8Async",
                code: res,
            });
        }

        Ok(())
    }

    #[must_use]
    pub const fn device_ptr(&self) -> CUdeviceptr {
        self.device_ptr
    }

    #[must_use]
    pub const fn len(&self) -> usize {
        self.len
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    #[must_use]
    pub const fn size_bytes(&self) -> usize {
        self.size_bytes
    }
}

impl<T: DeviceCopy> Drop for GpuBuffer<T> {
    fn drop(&mut self) {
        if self.device_ptr != 0 {
            let funcs = self.driver.functions();
            // SAFETY: Freeing memory allocated via cuMemAlloc on this device pointer.
            unsafe {
                (funcs.cu_mem_free)(self.device_ptr);
            }
            self.device_ptr = 0;
        }
    }
}
