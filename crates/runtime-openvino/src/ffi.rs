//! Low-level C FFI bindings and dynamic library loader for Intel OpenVINO.

#![allow(non_camel_case_types, non_snake_case)]

use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use crate::error::OpenVinoError;

#[repr(i32)]
#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq)]
pub enum ov_status_e {
    OK = 0,
    GENERAL_ERROR = -1,
    NOT_IMPLEMENTED = -2,
    NETWORK_NOT_LOADED = -3,
    PARAMETER_MISMATCH = -4,
    NOT_FOUND = -5,
    OUT_OF_BOUNDS = -6,
    UNEXPECTED = -7,
    REQUEST_BUSY = -8,
    RESULT_NOT_READY = -9,
    NOT_ALLOCATED = -10,
    INFER_NOT_STARTED = -11,
    NETWORK_NOT_READ = -12,
    INFER_CANCELLED = -13,
    INVALID_C_PARAM = -14,
    UNKNOWN_C_ERROR = -15,
    NOT_IMPLEMENT_C_METHOD = -16,
    UNKNOW_EXCEPTION = -17,
}

#[repr(u32)]
#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq)]
pub enum ov_element_type_e {
    DYNAMIC = 0,
    OV_BOOLEAN = 1,
    BF16 = 2,
    F16 = 3,
    F32 = 4,
    F64 = 5,
    I4 = 6,
    I8 = 7,
    I16 = 8,
    I32 = 9,
    I64 = 10,
    U1 = 11,
    U2 = 12,
    U3 = 13,
    U4 = 14,
    U6 = 15,
    U8 = 16,
    U16 = 17,
    U32 = 18,
    U64 = 19,
    NF4 = 20,
    F8E4M3 = 21,
    F8E5M2 = 22,
    STRING = 23,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ov_shape_t {
    pub rank: i64,
    pub dims: *mut i64,
}

/// `ov_dimension_t`: inclusive `[min, max]` limits of one dimension (also used as `ov_rank_t`).
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ov_dimension_t {
    pub min: i64,
    pub max: i64,
}

/// `ov_partial_shape_t`: a shape that may be partially or totally dynamic.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ov_partial_shape_t {
    pub rank: ov_dimension_t,
    pub dims: *mut ov_dimension_t,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ov_version_t {
    pub build_number: *const c_char,
    pub description: *const c_char,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ov_available_devices_t {
    pub devices: *mut *mut c_char,
    pub size: usize,
}

#[repr(C)]
pub struct ov_core_t {
    _private: [u8; 0],
}

#[repr(C)]
pub struct ov_model_t {
    _private: [u8; 0],
}

#[repr(C)]
pub struct ov_compiled_model_t {
    _private: [u8; 0],
}

#[repr(C)]
pub struct ov_infer_request_t {
    _private: [u8; 0],
}

#[repr(C)]
pub struct ov_tensor_t {
    _private: [u8; 0],
}

pub type FnOvCoreCreate = unsafe extern "C" fn(core: *mut *mut ov_core_t) -> ov_status_e;
pub type FnOvCoreFree = unsafe extern "C" fn(core: *mut ov_core_t);
pub type FnOvCoreGetAvailableDevices = unsafe extern "C" fn(
    core: *const ov_core_t,
    devices: *mut ov_available_devices_t,
) -> ov_status_e;
pub type FnOvAvailableDevicesFree = unsafe extern "C" fn(devices: *mut ov_available_devices_t);
pub type FnOvCoreReadModel = unsafe extern "C" fn(
    core: *const ov_core_t,
    model_path: *const c_char,
    bin_path: *const c_char,
    model: *mut *mut ov_model_t,
) -> ov_status_e;
pub type FnOvCoreReadModelFromMemoryBuffer = unsafe extern "C" fn(
    core: *const ov_core_t,
    model_str: *const c_char,
    str_len: usize,
    weights: *const ov_tensor_t,
    model: *mut *mut ov_model_t,
) -> ov_status_e;
pub type FnOvCoreCompileModel = unsafe extern "C" fn(
    core: *const ov_core_t,
    model: *const ov_model_t,
    device_name: *const c_char,
    property_args_size: usize,
    compiled_model: *mut *mut ov_compiled_model_t,
    ...
) -> ov_status_e;
pub type FnOvModelFree = unsafe extern "C" fn(model: *mut ov_model_t);
pub type FnOvModelReshapeInputByName = unsafe extern "C" fn(
    model: *const ov_model_t,
    tensor_name: *const c_char,
    partial_shape: ov_partial_shape_t,
) -> ov_status_e;
pub type FnOvPartialShapeCreateStatic = unsafe extern "C" fn(
    rank: i64,
    dims: *const i64,
    partial_shape: *mut ov_partial_shape_t,
) -> ov_status_e;
pub type FnOvPartialShapeFree = unsafe extern "C" fn(partial_shape: *mut ov_partial_shape_t);
pub type FnOvCompiledModelCreateInferRequest = unsafe extern "C" fn(
    compiled_model: *const ov_compiled_model_t,
    infer_request: *mut *mut ov_infer_request_t,
) -> ov_status_e;
pub type FnOvCompiledModelFree = unsafe extern "C" fn(compiled_model: *mut ov_compiled_model_t);
pub type FnOvInferRequestInfer =
    unsafe extern "C" fn(infer_request: *mut ov_infer_request_t) -> ov_status_e;
pub type FnOvInferRequestGetTensor = unsafe extern "C" fn(
    infer_request: *const ov_infer_request_t,
    tensor_name: *const c_char,
    tensor: *mut *mut ov_tensor_t,
) -> ov_status_e;
pub type FnOvInferRequestSetTensor = unsafe extern "C" fn(
    infer_request: *mut ov_infer_request_t,
    tensor_name: *const c_char,
    tensor: *const ov_tensor_t,
) -> ov_status_e;
pub type FnOvInferRequestGetInputTensorByIndex = unsafe extern "C" fn(
    infer_request: *const ov_infer_request_t,
    idx: usize,
    tensor: *mut *mut ov_tensor_t,
) -> ov_status_e;
pub type FnOvInferRequestGetOutputTensorByIndex = unsafe extern "C" fn(
    infer_request: *const ov_infer_request_t,
    idx: usize,
    tensor: *mut *mut ov_tensor_t,
) -> ov_status_e;
pub type FnOvInferRequestFree = unsafe extern "C" fn(infer_request: *mut ov_infer_request_t);
pub type FnOvTensorCreate = unsafe extern "C" fn(
    type_: ov_element_type_e,
    shape: ov_shape_t,
    tensor: *mut *mut ov_tensor_t,
) -> ov_status_e;
pub type FnOvTensorCreateFromHostPtr = unsafe extern "C" fn(
    type_: ov_element_type_e,
    shape: ov_shape_t,
    host_ptr: *mut std::ffi::c_void,
    tensor: *mut *mut ov_tensor_t,
) -> ov_status_e;
pub type FnOvTensorGetShape =
    unsafe extern "C" fn(tensor: *const ov_tensor_t, shape: *mut ov_shape_t) -> ov_status_e;
pub type FnOvTensorGetByteSize =
    unsafe extern "C" fn(tensor: *const ov_tensor_t, byte_size: *mut usize) -> ov_status_e;
pub type FnOvTensorData = unsafe extern "C" fn(
    tensor: *const ov_tensor_t,
    data: *mut *mut std::ffi::c_void,
) -> ov_status_e;
pub type FnOvTensorFree = unsafe extern "C" fn(tensor: *mut ov_tensor_t);
pub type FnOvShapeCreate =
    unsafe extern "C" fn(rank: i64, dims: *const i64, shape: *mut ov_shape_t) -> ov_status_e;
pub type FnOvShapeFree = unsafe extern "C" fn(shape: *mut ov_shape_t) -> ov_status_e;
pub type FnOvGetLastErrMsg = unsafe extern "C" fn() -> *const c_char;
pub type FnOvGetErrorInfo = unsafe extern "C" fn(status: ov_status_e) -> *const c_char;
pub type FnOvGetOpenvinoVersion = unsafe extern "C" fn(version: *mut ov_version_t) -> ov_status_e;
pub type FnOvVersionFree = unsafe extern "C" fn(version: *mut ov_version_t);

/// Dynamic function table holding loaded OpenVINO symbols.
pub struct OpenVinoLibrary {
    handle: *mut std::ffi::c_void,
    loaded_path: PathBuf,
    pub ov_core_create: FnOvCoreCreate,
    pub ov_core_free: FnOvCoreFree,
    pub ov_core_get_available_devices: FnOvCoreGetAvailableDevices,
    pub ov_available_devices_free: FnOvAvailableDevicesFree,
    pub ov_core_read_model: FnOvCoreReadModel,
    pub ov_core_read_model_from_memory_buffer: FnOvCoreReadModelFromMemoryBuffer,
    pub ov_core_compile_model: FnOvCoreCompileModel,
    pub ov_model_free: FnOvModelFree,
    pub ov_model_reshape_input_by_name: FnOvModelReshapeInputByName,
    pub ov_partial_shape_create_static: FnOvPartialShapeCreateStatic,
    pub ov_partial_shape_free: FnOvPartialShapeFree,
    pub ov_compiled_model_create_infer_request: FnOvCompiledModelCreateInferRequest,
    pub ov_compiled_model_free: FnOvCompiledModelFree,
    pub ov_infer_request_infer: FnOvInferRequestInfer,
    pub ov_infer_request_get_tensor: FnOvInferRequestGetTensor,
    pub ov_infer_request_set_tensor: FnOvInferRequestSetTensor,
    pub ov_infer_request_get_input_tensor_by_index: FnOvInferRequestGetInputTensorByIndex,
    pub ov_infer_request_get_output_tensor_by_index: FnOvInferRequestGetOutputTensorByIndex,
    pub ov_infer_request_free: FnOvInferRequestFree,
    pub ov_tensor_create: FnOvTensorCreate,
    pub ov_tensor_create_from_host_ptr: FnOvTensorCreateFromHostPtr,
    pub ov_tensor_get_shape: FnOvTensorGetShape,
    pub ov_tensor_get_byte_size: FnOvTensorGetByteSize,
    pub ov_tensor_data: FnOvTensorData,
    pub ov_tensor_free: FnOvTensorFree,
    pub ov_shape_create: FnOvShapeCreate,
    pub ov_shape_free: FnOvShapeFree,
    pub ov_get_last_err_msg: FnOvGetLastErrMsg,
    pub ov_get_error_info: FnOvGetErrorInfo,
    pub ov_get_openvino_version: FnOvGetOpenvinoVersion,
    pub ov_version_free: FnOvVersionFree,
}

unsafe impl Send for OpenVinoLibrary {}
unsafe impl Sync for OpenVinoLibrary {}

/// Known locations (and sonames) of the OpenVINO C library, in lookup order.
const LIBRARY_CANDIDATES: &[&str] = &[
    "/lib64/libopenvino_c.so.2510",
    "/usr/lib64/libopenvino_c.so.2510",
    "/lib64/libopenvino_c.so.2025.1.0",
    "/usr/lib64/libopenvino_c.so.2025.1.0",
    "/lib64/libopenvino_c.so",
    "/usr/lib64/libopenvino_c.so",
    "/usr/local/lib64/libopenvino_c.so",
    "libopenvino_c.so.2510",
    "libopenvino_c.so",
];

/// Finds the OpenVINO C library file on disk (`OPENVINO_LIB_PATH`, then known locations)
/// without loading it. Bare sonames are only resolvable by `dlopen`, so they are not reported.
#[must_use]
pub fn locate_library() -> Option<PathBuf> {
    std::env::var("OPENVINO_LIB_PATH")
        .ok()
        .map(PathBuf::from)
        .filter(|path| path.exists())
        .or_else(|| {
            LIBRARY_CANDIDATES
                .iter()
                .map(Path::new)
                .find(|path| path.is_absolute() && path.exists())
                .map(Path::to_path_buf)
        })
}

/// Release series the FFI layer is written against. The `ov_element_type_e` values mirrored in
/// this crate are the 2025.1 enum; other series renumber or extend it, which would silently
/// mis-type tensors, so any other series is rejected at load time.
pub const SUPPORTED_MAJOR_VERSION: u32 = 2025;

/// Whether an `ov_version_t::build_number` (for example `2025.1.0-18503-6fec06580ab`) belongs to
/// a supported release series.
#[must_use]
pub fn is_supported_version(build_number: &str) -> bool {
    build_number
        .trim()
        .split(['.', '-'])
        .next()
        .and_then(|major| major.parse::<u32>().ok())
        == Some(SUPPORTED_MAJOR_VERSION)
}

static GLOBAL_LIB: OnceLock<Result<Arc<OpenVinoLibrary>, OpenVinoError>> = OnceLock::new();

impl OpenVinoLibrary {
    /// Obtains or initializes the global singleton OpenVINO library instance.
    pub fn get_or_init() -> Result<Arc<Self>, OpenVinoError> {
        GLOBAL_LIB
            .get_or_init(|| Self::load().map(Arc::new))
            .clone()
    }

    /// Attempts to dynamically load OpenVINO from standard known paths or `OPENVINO_LIB_PATH`.
    pub fn load() -> Result<Self, OpenVinoError> {
        if let Ok(env_path) = std::env::var("OPENVINO_LIB_PATH") {
            let path = PathBuf::from(env_path);
            if path.exists() {
                return Self::load_from(&path);
            }
        }

        // A library that was found but is the wrong major version must be reported as such, not
        // masked by the generic "not found" at the end.
        let mut unsupported: Option<OpenVinoError> = None;
        let mut attempt = |path: &Path| match Self::load_from(path) {
            Ok(lib) => Some(lib),
            Err(error @ OpenVinoError::UnsupportedVersion(_)) => {
                unsupported = Some(error);
                None
            }
            Err(_) => None,
        };

        for &candidate in LIBRARY_CANDIDATES {
            let p = Path::new(candidate);
            if p.exists()
                && let Some(lib) = attempt(p)
            {
                return Ok(lib);
            }
        }

        // Try naked library name load via linker search
        #[cfg(unix)]
        {
            for name in ["libopenvino_c.so.2510", "libopenvino_c.so"] {
                if let Some(lib) = attempt(Path::new(name)) {
                    return Ok(lib);
                }
            }
        }

        #[cfg(windows)]
        {
            if let Some(lib) = attempt(Path::new("openvino_c.dll")) {
                return Ok(lib);
            }
        }

        Err(search_failure(unsupported))
    }

    /// Loads OpenVINO dynamically from an explicit file path.
    ///
    /// The library is rejected unless it reports a [`SUPPORTED_MAJOR_VERSION`] release. The
    /// `dlopen` handle is released on every failure path.
    pub fn load_from(path: &Path) -> Result<Self, OpenVinoError> {
        let (handle, loaded_path) = Self::open_library(path)?;
        let lib = match Self::bind_symbols(handle, loaded_path) {
            Ok(lib) => lib,
            Err(error) => {
                Self::close_library(handle);
                return Err(error);
            }
        };
        // From here `lib` owns the handle: `Drop` closes it if the version check fails.
        lib.ensure_supported_version()?;
        Ok(lib)
    }

    /// Resolves every required symbol. On error the caller still owns `handle`.
    fn bind_symbols(
        handle: *mut std::ffi::c_void,
        loaded_path: PathBuf,
    ) -> Result<Self, OpenVinoError> {
        macro_rules! load_sym {
            ($name:ident, $ty:ty) => {{
                let sym_name = stringify!($name);
                let ptr = unsafe { Self::get_symbol(handle, sym_name)? };
                unsafe { std::mem::transmute::<*mut std::ffi::c_void, $ty>(ptr) }
            }};
        }

        let lib = Self {
            handle,
            loaded_path,
            ov_core_create: load_sym!(ov_core_create, FnOvCoreCreate),
            ov_core_free: load_sym!(ov_core_free, FnOvCoreFree),
            ov_core_get_available_devices: load_sym!(
                ov_core_get_available_devices,
                FnOvCoreGetAvailableDevices
            ),
            ov_available_devices_free: load_sym!(
                ov_available_devices_free,
                FnOvAvailableDevicesFree
            ),
            ov_core_read_model: load_sym!(ov_core_read_model, FnOvCoreReadModel),
            ov_core_read_model_from_memory_buffer: load_sym!(
                ov_core_read_model_from_memory_buffer,
                FnOvCoreReadModelFromMemoryBuffer
            ),
            ov_core_compile_model: load_sym!(ov_core_compile_model, FnOvCoreCompileModel),
            ov_model_free: load_sym!(ov_model_free, FnOvModelFree),
            ov_model_reshape_input_by_name: load_sym!(
                ov_model_reshape_input_by_name,
                FnOvModelReshapeInputByName
            ),
            ov_partial_shape_create_static: load_sym!(
                ov_partial_shape_create_static,
                FnOvPartialShapeCreateStatic
            ),
            ov_partial_shape_free: load_sym!(ov_partial_shape_free, FnOvPartialShapeFree),
            ov_compiled_model_create_infer_request: load_sym!(
                ov_compiled_model_create_infer_request,
                FnOvCompiledModelCreateInferRequest
            ),
            ov_compiled_model_free: load_sym!(ov_compiled_model_free, FnOvCompiledModelFree),
            ov_infer_request_infer: load_sym!(ov_infer_request_infer, FnOvInferRequestInfer),
            ov_infer_request_get_tensor: load_sym!(
                ov_infer_request_get_tensor,
                FnOvInferRequestGetTensor
            ),
            ov_infer_request_set_tensor: load_sym!(
                ov_infer_request_set_tensor,
                FnOvInferRequestSetTensor
            ),
            ov_infer_request_get_input_tensor_by_index: load_sym!(
                ov_infer_request_get_input_tensor_by_index,
                FnOvInferRequestGetInputTensorByIndex
            ),
            ov_infer_request_get_output_tensor_by_index: load_sym!(
                ov_infer_request_get_output_tensor_by_index,
                FnOvInferRequestGetOutputTensorByIndex
            ),
            ov_infer_request_free: load_sym!(ov_infer_request_free, FnOvInferRequestFree),
            ov_tensor_create: load_sym!(ov_tensor_create, FnOvTensorCreate),
            ov_tensor_create_from_host_ptr: load_sym!(
                ov_tensor_create_from_host_ptr,
                FnOvTensorCreateFromHostPtr
            ),
            ov_tensor_get_shape: load_sym!(ov_tensor_get_shape, FnOvTensorGetShape),
            ov_tensor_get_byte_size: load_sym!(ov_tensor_get_byte_size, FnOvTensorGetByteSize),
            ov_tensor_data: load_sym!(ov_tensor_data, FnOvTensorData),
            ov_tensor_free: load_sym!(ov_tensor_free, FnOvTensorFree),
            ov_shape_create: load_sym!(ov_shape_create, FnOvShapeCreate),
            ov_shape_free: load_sym!(ov_shape_free, FnOvShapeFree),
            ov_get_last_err_msg: load_sym!(ov_get_last_err_msg, FnOvGetLastErrMsg),
            ov_get_error_info: load_sym!(ov_get_error_info, FnOvGetErrorInfo),
            ov_get_openvino_version: load_sym!(ov_get_openvino_version, FnOvGetOpenvinoVersion),
            ov_version_free: load_sym!(ov_version_free, FnOvVersionFree),
        };

        Ok(lib)
    }

    /// Asks the loaded library for its version and rejects unsupported release series.
    fn ensure_supported_version(&self) -> Result<(), OpenVinoError> {
        let mut version = ov_version_t {
            build_number: std::ptr::null(),
            description: std::ptr::null(),
        };
        // SAFETY: `version` is a valid out-parameter; the function table was just resolved.
        let status = unsafe { (self.ov_get_openvino_version)(&raw mut version) };
        if status != ov_status_e::OK {
            return Err(OpenVinoError::UnsupportedVersion(format!(
                "{} did not report its version (status {status:?})",
                self.loaded_path.display()
            )));
        }
        let build = if version.build_number.is_null() {
            String::new()
        } else {
            // SAFETY: non-null, NUL-terminated string owned by `version` until freed below.
            unsafe { CStr::from_ptr(version.build_number) }
                .to_string_lossy()
                .into_owned()
        };
        // SAFETY: `version` was filled by `ov_get_openvino_version`.
        unsafe { (self.ov_version_free)(&raw mut version) };

        if is_supported_version(&build) {
            Ok(())
        } else {
            Err(OpenVinoError::UnsupportedVersion(format!(
                "{} reports '{build}'; only {SUPPORTED_MAJOR_VERSION}.x is supported",
                self.loaded_path.display()
            )))
        }
    }

    #[must_use]
    pub fn loaded_path(&self) -> &Path {
        &self.loaded_path
    }

    /// Compiles a model with zero properties passed to variadic parameter.
    ///
    /// # Safety
    ///
    /// The caller must ensure that `core` and `model` are valid, non-null pointers to
    /// their respective OpenVINO objects, and that `compiled_model` is a valid, non-null
    /// pointer to a mutable pointer that will receive the compiled model handle.
    /// All pointers must remain valid for the duration of this call.
    pub unsafe fn compile_model_default(
        &self,
        core: *const ov_core_t,
        model: *const ov_model_t,
        device_name: *const c_char,
        compiled_model: *mut *mut ov_compiled_model_t,
    ) -> ov_status_e {
        unsafe { (self.ov_core_compile_model)(core, model, device_name, 0, compiled_model) }
    }

    /// Compiles a model passing up to three `(key, value)` string properties to the
    /// variadic `ov_core_compile_model`.
    ///
    /// `property_args_size` is the number of variadic *arguments* (two per property: key then
    /// value), as documented in `ov_core.h`. Properties beyond the third are ignored.
    ///
    /// # Safety
    ///
    /// Same contract as [`Self::compile_model_default`]. Every key and value must be a valid
    /// NUL-terminated string that outlives this call.
    pub unsafe fn compile_model_with_properties(
        &self,
        core: *const ov_core_t,
        model: *const ov_model_t,
        device_name: *const c_char,
        properties: &[(&CStr, &CStr)],
        compiled_model: *mut *mut ov_compiled_model_t,
    ) -> ov_status_e {
        let compile = self.ov_core_compile_model;
        match properties {
            [] => unsafe { compile(core, model, device_name, 0, compiled_model) },
            [(k0, v0)] => unsafe {
                compile(
                    core,
                    model,
                    device_name,
                    2,
                    compiled_model,
                    k0.as_ptr(),
                    v0.as_ptr(),
                )
            },
            [(k0, v0), (k1, v1)] => unsafe {
                compile(
                    core,
                    model,
                    device_name,
                    4,
                    compiled_model,
                    k0.as_ptr(),
                    v0.as_ptr(),
                    k1.as_ptr(),
                    v1.as_ptr(),
                )
            },
            [(k0, v0), (k1, v1), (k2, v2), ..] => unsafe {
                compile(
                    core,
                    model,
                    device_name,
                    6,
                    compiled_model,
                    k0.as_ptr(),
                    v0.as_ptr(),
                    k1.as_ptr(),
                    v1.as_ptr(),
                    k2.as_ptr(),
                    v2.as_ptr(),
                )
            },
        }
    }

    #[cfg(unix)]
    fn open_library(path: &Path) -> Result<(*mut std::ffi::c_void, PathBuf), OpenVinoError> {
        let path_str = path.to_str().ok_or_else(|| {
            OpenVinoError::LibraryNotFound(format!("path contains invalid UTF-8: {:?}", path))
        })?;
        let c_path = CString::new(path_str).map_err(|_| {
            OpenVinoError::LibraryNotFound(format!("path contains null byte: {:?}", path))
        })?;

        let handle = unsafe { libc::dlopen(c_path.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL) };
        if handle.is_null() {
            let err = unsafe {
                let err_ptr = libc::dlerror();
                if err_ptr.is_null() {
                    "unknown dlopen error".to_owned()
                } else {
                    CStr::from_ptr(err_ptr).to_string_lossy().into_owned()
                }
            };
            Err(OpenVinoError::LibraryNotFound(format!(
                "dlopen failed for {}: {}",
                path.display(),
                err
            )))
        } else {
            Ok((handle, path.to_path_buf()))
        }
    }

    #[cfg(unix)]
    unsafe fn get_symbol(
        handle: *mut std::ffi::c_void,
        name: &str,
    ) -> Result<*mut std::ffi::c_void, OpenVinoError> {
        let c_name = CString::new(name).map_err(|_| {
            OpenVinoError::SymbolNotFound(format!("symbol name contains null byte: {name}"))
        })?;
        let ptr = unsafe { libc::dlsym(handle, c_name.as_ptr()) };
        if ptr.is_null() {
            let err = unsafe {
                let err_ptr = libc::dlerror();
                if err_ptr.is_null() {
                    "unknown dlsym error".to_owned()
                } else {
                    CStr::from_ptr(err_ptr).to_string_lossy().into_owned()
                }
            };
            Err(OpenVinoError::SymbolNotFound(format!(
                "symbol {name} not found: {err}"
            )))
        } else {
            Ok(ptr)
        }
    }

    #[cfg(windows)]
    fn open_library(path: &Path) -> Result<(*mut std::ffi::c_void, PathBuf), OpenVinoError> {
        let path_str = path.to_str().ok_or_else(|| {
            OpenVinoError::LibraryNotFound(format!("path contains invalid UTF-8: {:?}", path))
        })?;
        let c_path = CString::new(path_str).map_err(|_| {
            OpenVinoError::LibraryNotFound(format!("path contains null byte: {:?}", path))
        })?;
        let h_module = unsafe {
            windows_sys::Win32::System::LibraryLoader::LoadLibraryA(c_path.as_ptr().cast())
        };
        if h_module.is_null() {
            Err(OpenVinoError::LibraryNotFound(format!(
                "LoadLibraryA failed for {}",
                path.display()
            )))
        } else {
            Ok((h_module.cast(), path.to_path_buf()))
        }
    }

    #[cfg(windows)]
    unsafe fn get_symbol(
        handle: *mut std::ffi::c_void,
        name: &str,
    ) -> Result<*mut std::ffi::c_void, OpenVinoError> {
        let c_name = CString::new(name).map_err(|_| {
            OpenVinoError::SymbolNotFound(format!("symbol name contains null byte: {name}"))
        })?;
        let ptr = unsafe {
            windows_sys::Win32::System::LibraryLoader::GetProcAddress(
                handle.cast(),
                c_name.as_ptr().cast(),
            )
        };
        if ptr.is_none() {
            Err(OpenVinoError::SymbolNotFound(format!(
                "GetProcAddress failed for {name}"
            )))
        } else {
            Ok(ptr.map_or(std::ptr::null_mut(), |p| p as *mut std::ffi::c_void))
        }
    }

    #[cfg(not(any(unix, windows)))]
    fn open_library(_path: &Path) -> Result<(*mut std::ffi::c_void, PathBuf), OpenVinoError> {
        Err(OpenVinoError::LibraryNotFound(
            "Platform not supported for dynamic loading".to_owned(),
        ))
    }

    #[cfg(not(any(unix, windows)))]
    unsafe fn get_symbol(
        _handle: *mut std::ffi::c_void,
        name: &str,
    ) -> Result<*mut std::ffi::c_void, OpenVinoError> {
        Err(OpenVinoError::SymbolNotFound(name.to_owned()))
    }
}

impl OpenVinoLibrary {
    /// Releases a handle obtained from `open_library` that was never wrapped in a library value.
    fn close_library(handle: *mut std::ffi::c_void) {
        if handle.is_null() {
            return;
        }
        #[cfg(unix)]
        // SAFETY: `handle` came from a successful `dlopen` and is not used afterwards.
        unsafe {
            libc::dlclose(handle);
        }
        #[cfg(windows)]
        // SAFETY: `handle` came from a successful `LoadLibraryA` and is not used afterwards.
        unsafe {
            windows_sys::Win32::Foundation::FreeLibrary(handle.cast());
        }
    }
}

impl Drop for OpenVinoLibrary {
    fn drop(&mut self) {
        Self::close_library(self.handle);
        self.handle = std::ptr::null_mut();
    }
}

/// The error for a search that loaded nothing: the last wrong-version library that was found,
/// or "not found" when no candidate was even present.
fn search_failure(unsupported: Option<OpenVinoError>) -> OpenVinoError {
    unsupported.unwrap_or_else(|| {
        OpenVinoError::LibraryNotFound("libopenvino_c not found in search paths".to_owned())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_found_but_unsupported_library_is_not_reported_as_missing() {
        let unsupported = OpenVinoError::UnsupportedVersion("2024.6.0".to_owned());
        assert!(matches!(
            search_failure(Some(unsupported)),
            OpenVinoError::UnsupportedVersion(msg) if msg == "2024.6.0"
        ));
        assert!(matches!(
            search_failure(None),
            OpenVinoError::LibraryNotFound(_)
        ));
    }

    #[test]
    fn accepts_only_the_2025_series() {
        assert!(is_supported_version(
            "2025.1.0-18503-6fec06580ab-releases/2025/1"
        ));
        assert!(is_supported_version("2025.4.1"));
        assert!(is_supported_version(" 2025.1.0 "));
        assert!(!is_supported_version("2024.6.0-17404-4c0f47d2335"));
        assert!(!is_supported_version("2026.0.0"));
        assert!(!is_supported_version("20251.0"));
        assert!(!is_supported_version("unknown"));
        assert!(!is_supported_version(""));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_library_without_the_symbols_is_rejected_cleanly() {
        // libm is always present and has none of the OpenVINO symbols: the load must fail with
        // SymbolNotFound (and release the dlopen handle it opened) instead of returning a table.
        let result = OpenVinoLibrary::load_from(Path::new("libm.so.6"));
        assert!(matches!(result, Err(OpenVinoError::SymbolNotFound(_))));
    }

    #[test]
    fn the_installed_runtime_is_accepted_or_absent() {
        match OpenVinoLibrary::load() {
            Ok(lib) => assert!(lib.loaded_path().exists() || lib.loaded_path().is_relative()),
            Err(OpenVinoError::LibraryNotFound(_) | OpenVinoError::UnsupportedVersion(_)) => {}
            Err(other) => panic!("unexpected load error: {other}"),
        }
    }
}
