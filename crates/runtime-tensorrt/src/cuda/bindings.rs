use std::ffi::{CString, c_char, c_int, c_uint, c_void};
use std::path::Path;
use std::sync::Arc;

use crate::error::CudaError;

pub type CUresult = c_int;
pub type CUdevice = c_int;
pub type CUcontext = *mut c_void;
pub type CUdeviceptr = usize;
pub type CUstream = *mut c_void;

pub const CUDA_SUCCESS: CUresult = 0;
pub const CUDA_ERROR_NOT_READY: CUresult = 600;

pub const CU_DEVICE_ATTRIBUTE_COMPUTE_CAPABILITY_MAJOR: c_int = 75;
pub const CU_DEVICE_ATTRIBUTE_COMPUTE_CAPABILITY_MINOR: c_int = 76;
pub const CU_DEVICE_ATTRIBUTE_MULTIPROCESSOR_COUNT: c_int = 16;

const CUDA_DRIVER_CANDIDATES: &[&str] = &[
    #[cfg(unix)]
    "/lib64/libcuda.so.1",
    #[cfg(unix)]
    "/usr/lib64/libcuda.so.1",
    #[cfg(unix)]
    "/usr/lib/x86_64-linux-gnu/libcuda.so.1",
    #[cfg(unix)]
    "libcuda.so.1",
    #[cfg(unix)]
    "libcuda.so",
    #[cfg(windows)]
    "nvcuda.dll",
    #[cfg(windows)]
    "C:\\Windows\\System32\\nvcuda.dll",
];

pub struct CudaDriverFunctions {
    pub cu_init: unsafe extern "C" fn(flags: c_uint) -> CUresult,
    pub cu_device_get: unsafe extern "C" fn(device: *mut CUdevice, ordinal: c_int) -> CUresult,
    pub cu_device_get_name:
        unsafe extern "C" fn(name: *mut c_char, len: c_int, dev: CUdevice) -> CUresult,
    pub cu_device_get_count: unsafe extern "C" fn(count: *mut c_int) -> CUresult,
    pub cu_device_get_attribute:
        unsafe extern "C" fn(pi: *mut c_int, attrib: c_int, dev: CUdevice) -> CUresult,
    pub cu_ctx_create:
        unsafe extern "C" fn(pctx: *mut CUcontext, flags: c_uint, dev: CUdevice) -> CUresult,
    pub cu_ctx_destroy: unsafe extern "C" fn(ctx: CUcontext) -> CUresult,
    pub cu_ctx_set_current: unsafe extern "C" fn(ctx: CUcontext) -> CUresult,
    pub cu_mem_alloc: unsafe extern "C" fn(dptr: *mut CUdeviceptr, bytesize: usize) -> CUresult,
    pub cu_mem_free: unsafe extern "C" fn(dptr: CUdeviceptr) -> CUresult,
    pub cu_memcpy_htod_async: unsafe extern "C" fn(
        dst_device: CUdeviceptr,
        src_host: *const c_void,
        byte_count: usize,
        stream: CUstream,
    ) -> CUresult,
    pub cu_memcpy_dtoh_async: unsafe extern "C" fn(
        dst_host: *mut c_void,
        src_device: CUdeviceptr,
        byte_count: usize,
        stream: CUstream,
    ) -> CUresult,
    pub cu_memcpy_dtod_async: unsafe extern "C" fn(
        dst_device: CUdeviceptr,
        src_device: CUdeviceptr,
        byte_count: usize,
        stream: CUstream,
    ) -> CUresult,
    pub cu_memset_d8_async: unsafe extern "C" fn(
        dst_device: CUdeviceptr,
        value: std::ffi::c_uchar,
        byte_count: usize,
        stream: CUstream,
    ) -> CUresult,
    pub cu_stream_create: unsafe extern "C" fn(stream: *mut CUstream, flags: c_uint) -> CUresult,
    pub cu_stream_destroy: unsafe extern "C" fn(stream: CUstream) -> CUresult,
    pub cu_stream_synchronize: unsafe extern "C" fn(stream: CUstream) -> CUresult,
    pub cu_stream_query: unsafe extern "C" fn(stream: CUstream) -> CUresult,
}

pub struct CudaDriver {
    handle: *mut c_void,
    functions: CudaDriverFunctions,
}

// SAFETY: CUDA driver functions and handle are safe to share across threads.
unsafe impl Send for CudaDriver {}
// SAFETY: CUDA driver functions and handle are safe to share across threads.
unsafe impl Sync for CudaDriver {}

impl CudaDriver {
    /// Attempts to dynamically load the NVIDIA CUDA Driver library.
    pub fn load() -> Result<Arc<Self>, CudaError> {
        let (handle, path_used) = Self::open_library()?;
        let functions = match Self::load_symbols(handle) {
            Ok(funcs) => funcs,
            Err(e) => {
                Self::close_handle(handle);
                return Err(e);
            }
        };

        // SAFETY: cu_init is a dynamically loaded CUDA Driver entrypoint.
        let init_res = unsafe { (functions.cu_init)(0) };
        if init_res != CUDA_SUCCESS {
            Self::close_handle(handle);
            return Err(CudaError::DriverCallFailed {
                function: "cuInit",
                code: init_res,
            });
        }

        let _ = path_used;
        Ok(Arc::new(Self { handle, functions }))
    }

    #[must_use]
    pub const fn functions(&self) -> &CudaDriverFunctions {
        &self.functions
    }

    fn open_library() -> Result<(*mut c_void, &'static str), CudaError> {
        #[cfg(target_os = "windows")]
        {
            use windows_sys::Win32::System::LibraryLoader::LoadLibraryA;
            for &candidate in CUDA_DRIVER_CANDIDATES {
                let mut null_terminated = candidate.as_bytes().to_vec();
                null_terminated.push(0);
                // SAFETY: Calling LoadLibraryA with valid null-terminated ASCII string.
                let handle = unsafe { LoadLibraryA(null_terminated.as_ptr()) };
                if !handle.is_null() {
                    return Ok((handle.cast(), candidate));
                }
            }
            Err(CudaError::DriverNotFound(
                "nvcuda.dll was not found on the host system".to_owned(),
            ))
        }
        #[cfg(unix)]
        {
            for &candidate in CUDA_DRIVER_CANDIDATES {
                if Path::new(candidate).exists()
                    && let Ok(c_str) = CString::new(candidate)
                {
                    // SAFETY: Probing dlopen with RTLD_LAZY | RTLD_LOCAL.
                    let handle =
                        unsafe { libc::dlopen(c_str.as_ptr(), libc::RTLD_LAZY | libc::RTLD_LOCAL) };
                    if !handle.is_null() {
                        return Ok((handle, candidate));
                    }
                }
            }
            // Fallback: try loading by library name directly via system loader
            if let Ok(c_str) = CString::new("libcuda.so.1") {
                // SAFETY: Probing dlopen with RTLD_LAZY | RTLD_LOCAL.
                let handle =
                    unsafe { libc::dlopen(c_str.as_ptr(), libc::RTLD_LAZY | libc::RTLD_LOCAL) };
                if !handle.is_null() {
                    return Ok((handle, "libcuda.so.1"));
                }
            }
            Err(CudaError::DriverNotFound(
                "libcuda.so.1 was not found on the host system".to_owned(),
            ))
        }
    }

    fn close_handle(handle: *mut c_void) {
        if handle.is_null() {
            return;
        }
        #[cfg(target_os = "windows")]
        {
            use windows_sys::Win32::Foundation::FreeLibrary;
            // SAFETY: FreeLibrary on a non-null module handle.
            unsafe { FreeLibrary(handle.cast()) };
        }
        #[cfg(unix)]
        {
            // SAFETY: Closing library handle opened via dlopen.
            unsafe { libc::dlclose(handle) };
        }
    }

    fn load_symbol<T>(handle: *mut c_void, name: &'static str) -> Result<T, CudaError> {
        let sym_ptr = Self::resolve_symbol(handle, name)?;
        // SAFETY: Casting function pointer loaded from dynamic library to expected signature T.
        unsafe { Ok(std::mem::transmute_copy::<*mut c_void, T>(&sym_ptr)) }
    }

    fn resolve_symbol(handle: *mut c_void, name: &'static str) -> Result<*mut c_void, CudaError> {
        #[cfg(target_os = "windows")]
        {
            use windows_sys::Win32::System::LibraryLoader::GetProcAddress;
            let mut null_terminated = name.as_bytes().to_vec();
            null_terminated.push(0);
            // SAFETY: Calling GetProcAddress with valid null-terminated symbol name.
            let proc = unsafe { GetProcAddress(handle.cast(), null_terminated.as_ptr()) };
            if proc.is_none() {
                return Err(CudaError::SymbolNotFound(name));
            }
            // SAFETY: Transmuting FARPROC option to raw pointer.
            unsafe { Ok(std::mem::transmute(proc)) }
        }
        #[cfg(unix)]
        {
            let c_str = CString::new(name).map_err(|_| CudaError::SymbolNotFound(name))?;
            // SAFETY: Calling dlsym on a valid dynamic library handle.
            let sym = unsafe { libc::dlsym(handle, c_str.as_ptr()) };
            if sym.is_null() {
                return Err(CudaError::SymbolNotFound(name));
            }
            Ok(sym)
        }
    }

    fn load_symbols(handle: *mut c_void) -> Result<CudaDriverFunctions, CudaError> {
        // Resolve symbols, using v2 variants where standard in CUDA Driver API
        let cu_init = Self::load_symbol(handle, "cuInit")?;
        let cu_device_get = Self::load_symbol(handle, "cuDeviceGet")?;
        let cu_device_get_name = Self::load_symbol(handle, "cuDeviceGetName")?;
        let cu_device_get_count = Self::load_symbol(handle, "cuDeviceGetCount")?;
        let cu_device_get_attribute = Self::load_symbol(handle, "cuDeviceGetAttribute")?;

        let cu_ctx_create = Self::load_symbol(handle, "cuCtxCreate_v2")
            .or_else(|_| Self::load_symbol(handle, "cuCtxCreate"))?;
        let cu_ctx_destroy = Self::load_symbol(handle, "cuCtxDestroy_v2")
            .or_else(|_| Self::load_symbol(handle, "cuCtxDestroy"))?;
        let cu_ctx_set_current = Self::load_symbol(handle, "cuCtxSetCurrent")?;

        let cu_mem_alloc = Self::load_symbol(handle, "cuMemAlloc_v2")
            .or_else(|_| Self::load_symbol(handle, "cuMemAlloc"))?;
        let cu_mem_free = Self::load_symbol(handle, "cuMemFree_v2")
            .or_else(|_| Self::load_symbol(handle, "cuMemFree"))?;
        let cu_memcpy_htod_async = Self::load_symbol(handle, "cuMemcpyHtoDAsync_v2")
            .or_else(|_| Self::load_symbol(handle, "cuMemcpyHtoDAsync"))?;
        let cu_memcpy_dtoh_async = Self::load_symbol(handle, "cuMemcpyDtoHAsync_v2")
            .or_else(|_| Self::load_symbol(handle, "cuMemcpyDtoHAsync"))?;
        let cu_memcpy_dtod_async = Self::load_symbol(handle, "cuMemcpyDtoDAsync_v2")
            .or_else(|_| Self::load_symbol(handle, "cuMemcpyDtoDAsync"))?;
        let cu_memset_d8_async = Self::load_symbol(handle, "cuMemsetD8Async")
            .or_else(|_| Self::load_symbol(handle, "cuMemsetD8Async_v2"))?;

        let cu_stream_create = Self::load_symbol(handle, "cuStreamCreate")?;
        let cu_stream_destroy = Self::load_symbol(handle, "cuStreamDestroy_v2")
            .or_else(|_| Self::load_symbol(handle, "cuStreamDestroy"))?;
        let cu_stream_synchronize = Self::load_symbol(handle, "cuStreamSynchronize")?;
        let cu_stream_query = Self::load_symbol(handle, "cuStreamQuery")?;

        Ok(CudaDriverFunctions {
            cu_init,
            cu_device_get,
            cu_device_get_name,
            cu_device_get_count,
            cu_device_get_attribute,
            cu_ctx_create,
            cu_ctx_destroy,
            cu_ctx_set_current,
            cu_mem_alloc,
            cu_mem_free,
            cu_memcpy_htod_async,
            cu_memcpy_dtoh_async,
            cu_memcpy_dtod_async,
            cu_memset_d8_async,
            cu_stream_create,
            cu_stream_destroy,
            cu_stream_synchronize,
            cu_stream_query,
        })
    }
}

impl Drop for CudaDriver {
    fn drop(&mut self) {
        Self::close_handle(self.handle);
    }
}

impl std::fmt::Debug for CudaDriver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CudaDriver")
            .field("handle", &self.handle)
            .finish()
    }
}

/// Probes whether the NVIDIA CUDA driver is present on the host system.
#[must_use]
pub fn is_cuda_driver_available() -> bool {
    CudaDriver::load().is_ok()
}
