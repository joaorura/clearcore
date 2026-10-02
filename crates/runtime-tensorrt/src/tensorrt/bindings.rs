use std::ffi::{CString, c_int, c_void};
use std::path::Path;
use std::sync::Arc;

use crate::error::TensorRtError;

const TENSORRT_LIB_CANDIDATES: &[&str] = &[
    #[cfg(unix)]
    "/usr/lib64/libnvinfer.so.11",
    #[cfg(unix)]
    "/usr/lib64/libnvinfer.so.10",
    #[cfg(unix)]
    "/usr/lib64/libnvinfer.so",
    #[cfg(unix)]
    "/usr/lib/x86_64-linux-gnu/libnvinfer.so.11",
    #[cfg(unix)]
    "/usr/lib/x86_64-linux-gnu/libnvinfer.so",
    #[cfg(unix)]
    "/usr/local/cuda/lib64/libnvinfer.so",
    #[cfg(unix)]
    "libnvinfer.so.11",
    #[cfg(unix)]
    "libnvinfer.so",
    #[cfg(windows)]
    "nvinfer_11.dll",
    #[cfg(windows)]
    "nvinfer.dll",
    #[cfg(windows)]
    "C:\\Program Files\\NVIDIA GPU Computing Toolkit\\CUDA\\v13.0\\bin\\nvinfer_11.dll",
];

pub struct TensorRtFunctions {
    pub get_infer_lib_version: unsafe extern "C" fn() -> c_int,
    pub get_infer_lib_major_version: unsafe extern "C" fn() -> c_int,
    pub get_infer_lib_minor_version: unsafe extern "C" fn() -> c_int,
    pub get_infer_lib_patch_version: unsafe extern "C" fn() -> c_int,
    pub get_infer_lib_build_version: unsafe extern "C" fn() -> c_int,
    pub create_infer_runtime_internal:
        Option<unsafe extern "C" fn(logger: *mut c_void, version: c_int) -> *mut c_void>,
}

pub struct TensorRtLibrary {
    handle: *mut c_void,
    functions: TensorRtFunctions,
    version_string: String,
    path_loaded: String,
}

// SAFETY: TensorRT functions and handles are thread-safe.
unsafe impl Send for TensorRtLibrary {}
// SAFETY: TensorRT library wrapper is thread-safe.
unsafe impl Sync for TensorRtLibrary {}

impl TensorRtLibrary {
    /// Attempts to dynamically load the TensorRT runtime library (`libnvinfer`).
    pub fn load() -> Result<Arc<Self>, TensorRtError> {
        let (handle, path) = Self::open_library()?;
        let functions = match Self::load_symbols(handle) {
            Ok(funcs) => funcs,
            Err(e) => {
                Self::close_handle(handle);
                return Err(e);
            }
        };

        // SAFETY: Calling get_infer_lib_* functions loaded from valid libnvinfer.
        let major = unsafe { (functions.get_infer_lib_major_version)() };
        let minor = unsafe { (functions.get_infer_lib_minor_version)() };
        let patch = unsafe { (functions.get_infer_lib_patch_version)() };
        let build = unsafe { (functions.get_infer_lib_build_version)() };

        let version_string = format!("{major}.{minor}.{patch}.{build}");

        Ok(Arc::new(Self {
            handle,
            functions,
            version_string,
            path_loaded: path,
        }))
    }

    #[must_use]
    pub fn version_string(&self) -> &str {
        &self.version_string
    }

    #[must_use]
    pub fn path_loaded(&self) -> &str {
        &self.path_loaded
    }

    #[must_use]
    pub const fn functions(&self) -> &TensorRtFunctions {
        &self.functions
    }

    fn open_library() -> Result<(*mut c_void, String), TensorRtError> {
        // 1. Check environment variable override
        if let Ok(env_path) = std::env::var("CLEARCORE_TENSORRT_PATH")
            && Path::new(&env_path).exists()
            && let Ok((handle, path)) = Self::try_load_path(&env_path)
        {
            return Ok((handle, path));
        }
        if let Ok(env_path) = std::env::var("TENSORRT_LIB_PATH")
            && Path::new(&env_path).exists()
            && let Ok((handle, path)) = Self::try_load_path(&env_path)
        {
            return Ok((handle, path));
        }

        // 2. Iterate through known candidate locations
        for &candidate in TENSORRT_LIB_CANDIDATES {
            if Path::new(candidate).exists()
                && let Ok((handle, path)) = Self::try_load_path(candidate)
            {
                return Ok((handle, path));
            }
        }

        // 3. System dynamic linker search by name
        #[cfg(unix)]
        {
            for name in &["libnvinfer.so.11", "libnvinfer.so"] {
                if let Ok((handle, path)) = Self::try_load_path(name) {
                    return Ok((handle, path));
                }
            }
        }
        #[cfg(windows)]
        {
            for name in &["nvinfer_11.dll", "nvinfer.dll"] {
                if let Ok((handle, path)) = Self::try_load_path(name) {
                    return Ok((handle, path));
                }
            }
        }

        Err(TensorRtError::LibraryNotFound(
            "libnvinfer.so / nvinfer.dll was not found on the host system".to_owned(),
        ))
    }

    fn try_load_path(path: &str) -> Result<(*mut c_void, String), TensorRtError> {
        #[cfg(target_os = "windows")]
        {
            use windows_sys::Win32::System::LibraryLoader::LoadLibraryA;
            let mut null_terminated = path.as_bytes().to_vec();
            null_terminated.push(0);
            // SAFETY: Calling LoadLibraryA with valid null-terminated ASCII string.
            let handle = unsafe { LoadLibraryA(null_terminated.as_ptr()) };
            if handle.is_null() {
                Err(TensorRtError::LibraryNotFound(format!(
                    "LoadLibraryA failed for {path}"
                )))
            } else {
                Ok((handle.cast(), path.to_owned()))
            }
        }
        #[cfg(unix)]
        {
            let c_str = CString::new(path).map_err(|_| {
                TensorRtError::LibraryNotFound(format!("Invalid path string: {path}"))
            })?;
            // SAFETY: dlopen with RTLD_LAZY | RTLD_LOCAL.
            let handle =
                unsafe { libc::dlopen(c_str.as_ptr(), libc::RTLD_LAZY | libc::RTLD_LOCAL) };
            if handle.is_null() {
                Err(TensorRtError::LibraryNotFound(format!(
                    "dlopen failed for {path}"
                )))
            } else {
                Ok((handle, path.to_owned()))
            }
        }
    }

    fn close_handle(handle: *mut c_void) {
        if handle.is_null() {
            return;
        }
        #[cfg(target_os = "windows")]
        {
            use windows_sys::Win32::System::LibraryLoader::FreeLibrary;
            // SAFETY: FreeLibrary on valid module handle.
            unsafe { FreeLibrary(handle.cast()) };
        }
        #[cfg(unix)]
        {
            // SAFETY: dlclose on valid dynamic library handle.
            unsafe { libc::dlclose(handle) };
        }
    }

    fn resolve_symbol(
        handle: *mut c_void,
        name: &'static str,
    ) -> Result<*mut c_void, TensorRtError> {
        #[cfg(target_os = "windows")]
        {
            use windows_sys::Win32::System::LibraryLoader::GetProcAddress;
            let mut null_terminated = name.as_bytes().to_vec();
            null_terminated.push(0);
            // SAFETY: GetProcAddress with valid null-terminated symbol name.
            let proc = unsafe { GetProcAddress(handle.cast(), null_terminated.as_ptr()) };
            if proc.is_none() {
                return Err(TensorRtError::SymbolNotFound(name));
            }
            // SAFETY: Transmute FARPROC option to raw pointer.
            unsafe { Ok(std::mem::transmute(proc)) }
        }
        #[cfg(unix)]
        {
            let c_str = CString::new(name).map_err(|_| TensorRtError::SymbolNotFound(name))?;
            // SAFETY: dlsym on valid library handle.
            let sym = unsafe { libc::dlsym(handle, c_str.as_ptr()) };
            if sym.is_null() {
                return Err(TensorRtError::SymbolNotFound(name));
            }
            Ok(sym)
        }
    }

    fn load_symbol<T>(handle: *mut c_void, name: &'static str) -> Result<T, TensorRtError> {
        let sym_ptr = Self::resolve_symbol(handle, name)?;
        // SAFETY: Casting function pointer to expected signature T.
        unsafe { Ok(std::mem::transmute_copy::<*mut c_void, T>(&sym_ptr)) }
    }

    fn load_symbols(handle: *mut c_void) -> Result<TensorRtFunctions, TensorRtError> {
        let get_infer_lib_version = Self::load_symbol(handle, "getInferLibVersion")?;
        let get_infer_lib_major_version = Self::load_symbol(handle, "getInferLibMajorVersion")?;
        let get_infer_lib_minor_version = Self::load_symbol(handle, "getInferLibMinorVersion")?;
        let get_infer_lib_patch_version = Self::load_symbol(handle, "getInferLibPatchVersion")?;
        let get_infer_lib_build_version = Self::load_symbol(handle, "getInferLibBuildVersion")?;

        let create_infer_runtime_internal =
            Self::load_symbol(handle, "createInferRuntime_INTERNAL").ok();

        Ok(TensorRtFunctions {
            get_infer_lib_version,
            get_infer_lib_major_version,
            get_infer_lib_minor_version,
            get_infer_lib_patch_version,
            get_infer_lib_build_version,
            create_infer_runtime_internal,
        })
    }
}

impl Drop for TensorRtLibrary {
    fn drop(&mut self) {
        Self::close_handle(self.handle);
    }
}

impl std::fmt::Debug for TensorRtLibrary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TensorRtLibrary")
            .field("version_string", &self.version_string)
            .field("path_loaded", &self.path_loaded)
            .finish()
    }
}

/// Finds the TensorRT library file on disk (environment overrides, then known locations)
/// without loading it. Bare soname candidates are only resolvable by `dlopen`, so they are
/// not reported here.
#[must_use]
pub fn locate_library() -> Option<String> {
    ["CLEARCORE_TENSORRT_PATH", "TENSORRT_LIB_PATH"]
        .iter()
        .filter_map(|var| std::env::var(var).ok())
        .find(|path| Path::new(path).exists())
        .or_else(|| {
            TENSORRT_LIB_CANDIDATES
                .iter()
                .find(|candidate| {
                    Path::new(candidate).is_absolute() && Path::new(candidate).exists()
                })
                .map(|candidate| (*candidate).to_owned())
        })
}

/// Probes whether the TensorRT library is available on the system.
#[must_use]
pub fn is_tensorrt_available() -> bool {
    TensorRtLibrary::load().is_ok()
}
