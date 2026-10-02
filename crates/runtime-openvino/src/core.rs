//! Safe OpenVINO Core abstraction.

use std::ffi::{CStr, CString};
use std::path::Path;
use std::sync::Arc;

use crate::error::{OpenVinoError, check_status};
use crate::ffi::{OpenVinoLibrary, ov_available_devices_t, ov_core_t, ov_version_t};
use crate::model::{OpenVinoCompiledModel, OpenVinoModel};
use crate::tensor::OpenVinoTensor;

/// Maximum number of compile properties supported by the fixed-arity variadic call.
pub const MAX_COMPILE_PROPERTIES: usize = 3;

/// Safe wrapper around OpenVINO runtime core (`ov_core_t`).
pub struct OpenVinoCore {
    ptr: *mut ov_core_t,
    lib: Arc<OpenVinoLibrary>,
}

unsafe impl Send for OpenVinoCore {}
unsafe impl Sync for OpenVinoCore {}

impl OpenVinoCore {
    /// Initializes OpenVINO core by finding and loading the native runtime library.
    pub fn new() -> Result<Self, OpenVinoError> {
        let lib = OpenVinoLibrary::get_or_init()?;
        Self::new_with_lib(lib)
    }

    /// Initializes OpenVINO core using a specific loaded library instance.
    pub fn new_with_lib(lib: Arc<OpenVinoLibrary>) -> Result<Self, OpenVinoError> {
        let mut core_ptr = std::ptr::null_mut();
        let status = unsafe { (lib.ov_core_create)(&mut core_ptr) };
        check_status(status, &lib).map_err(|e| match e {
            OpenVinoError::StatusError { message, .. } => {
                OpenVinoError::CoreCreationFailed(message)
            }
            other => other,
        })?;

        if core_ptr.is_null() {
            return Err(OpenVinoError::CoreCreationFailed(
                "ov_core_create returned null pointer".to_owned(),
            ));
        }

        Ok(Self { ptr: core_ptr, lib })
    }

    /// Queries the OpenVINO runtime version string.
    pub fn version(&self) -> Result<String, OpenVinoError> {
        let mut version = ov_version_t {
            build_number: std::ptr::null(),
            description: std::ptr::null(),
        };

        let status = unsafe { (self.lib.ov_get_openvino_version)(&mut version) };
        check_status(status, &self.lib)?;

        let build = if version.build_number.is_null() {
            "unknown".to_owned()
        } else {
            unsafe { CStr::from_ptr(version.build_number) }
                .to_string_lossy()
                .into_owned()
        };

        let desc = if version.description.is_null() {
            String::new()
        } else {
            unsafe { CStr::from_ptr(version.description) }
                .to_string_lossy()
                .into_owned()
        };

        unsafe {
            (self.lib.ov_version_free)(&mut version);
        }

        if desc.is_empty() {
            Ok(build)
        } else {
            Ok(format!("{build} ({desc})"))
        }
    }

    /// Returns a list of hardware devices available for inference (e.g. `["CPU", "GPU", "NPU"]`).
    pub fn available_devices(&self) -> Result<Vec<String>, OpenVinoError> {
        let mut devices = ov_available_devices_t {
            devices: std::ptr::null_mut(),
            size: 0,
        };

        let status = unsafe { (self.lib.ov_core_get_available_devices)(self.ptr, &mut devices) };
        check_status(status, &self.lib)?;

        if devices.size == 0 || devices.devices.is_null() {
            return Ok(Vec::new());
        }

        let slice = unsafe { std::slice::from_raw_parts(devices.devices, devices.size) };
        let mut list = Vec::with_capacity(devices.size);

        for &c_str_ptr in slice {
            if !c_str_ptr.is_null() {
                let s = unsafe { CStr::from_ptr(c_str_ptr) }
                    .to_string_lossy()
                    .into_owned();
                list.push(s);
            }
        }

        unsafe {
            (self.lib.ov_available_devices_free)(&mut devices);
        }

        Ok(list)
    }

    /// Checks if a specific device (e.g., "CPU", "GPU", "NPU") is available.
    pub fn is_device_available(&self, device_name: &str) -> bool {
        self.available_devices()
            .map(|devs| devs.iter().any(|d| d.eq_ignore_ascii_case(device_name)))
            .unwrap_or(false)
    }

    /// Reads a model from disk.
    ///
    /// For `.onnx`, `bin_path` should be `None`.
    /// For OpenVINO IR `.xml`, `bin_path` can point to the corresponding `.bin` file
    /// or be `None` to auto-locate the `.bin` with the same base name.
    pub fn read_model_from_file(
        &self,
        model_path: impl AsRef<Path>,
        bin_path: Option<impl AsRef<Path>>,
    ) -> Result<OpenVinoModel, OpenVinoError> {
        let model_str = model_path
            .as_ref()
            .to_str()
            .ok_or_else(|| OpenVinoError::InvalidInput("model path is invalid UTF-8".to_owned()))?;
        let c_model_path = CString::new(model_str)
            .map_err(|_| OpenVinoError::InvalidInput("model path contains null byte".to_owned()))?;

        let c_bin_path = match bin_path {
            Some(p) => {
                let bin_str = p.as_ref().to_str().ok_or_else(|| {
                    OpenVinoError::InvalidInput("bin path is invalid UTF-8".to_owned())
                })?;
                Some(CString::new(bin_str).map_err(|_| {
                    OpenVinoError::InvalidInput("bin path contains null byte".to_owned())
                })?)
            }
            None => None,
        };

        let mut model_ptr = std::ptr::null_mut();
        let bin_ptr = c_bin_path.as_ref().map_or(std::ptr::null(), |p| p.as_ptr());

        let status = unsafe {
            (self.lib.ov_core_read_model)(self.ptr, c_model_path.as_ptr(), bin_ptr, &mut model_ptr)
        };
        check_status(status, &self.lib).map_err(|e| match e {
            OpenVinoError::StatusError { message, .. } => OpenVinoError::ModelReadFailed(message),
            other => other,
        })?;

        OpenVinoModel::from_raw(model_ptr, self.lib.clone())
    }

    /// Reads a model directly from an in-memory buffer (XML string or ONNX serialized bytes).
    pub fn read_model_from_memory(
        &self,
        model_bytes: &[u8],
        weights: Option<&OpenVinoTensor>,
    ) -> Result<OpenVinoModel, OpenVinoError> {
        let mut model_ptr = std::ptr::null_mut();
        let weights_ptr = weights.map_or(std::ptr::null(), OpenVinoTensor::as_ptr);

        let status = unsafe {
            (self.lib.ov_core_read_model_from_memory_buffer)(
                self.ptr,
                model_bytes.as_ptr().cast(),
                model_bytes.len(),
                weights_ptr,
                &mut model_ptr,
            )
        };
        check_status(status, &self.lib).map_err(|e| match e {
            OpenVinoError::StatusError { message, .. } => OpenVinoError::ModelReadFailed(message),
            other => other,
        })?;

        OpenVinoModel::from_raw(model_ptr, self.lib.clone())
    }

    /// Compiles a model for a specific target device ("CPU", "GPU", or "NPU").
    pub fn compile_model(
        &self,
        model: &OpenVinoModel,
        device_name: &str,
    ) -> Result<OpenVinoCompiledModel, OpenVinoError> {
        let c_device = CString::new(device_name).map_err(|_| {
            OpenVinoError::InvalidInput(format!("device name contains null byte: {device_name}"))
        })?;

        let mut compiled_ptr = std::ptr::null_mut();
        let status = unsafe {
            self.lib.compile_model_default(
                self.ptr,
                model.as_ptr(),
                c_device.as_ptr(),
                &mut compiled_ptr,
            )
        };
        check_status(status, &self.lib).map_err(|e| match e {
            OpenVinoError::StatusError { message, .. } => OpenVinoError::ModelCompilationFailed(
                format!("Device {device_name} compilation error: {message}"),
            ),
            other => other,
        })?;

        OpenVinoCompiledModel::from_raw(compiled_ptr, self.lib.clone())
    }

    /// Compiles a model for a device with string properties (at most three), e.g.
    /// `("INFERENCE_PRECISION_HINT", "f16")` or `("PERFORMANCE_HINT", "LATENCY")`.
    pub fn compile_model_with_properties(
        &self,
        model: &OpenVinoModel,
        device_name: &str,
        properties: &[(&str, &str)],
    ) -> Result<OpenVinoCompiledModel, OpenVinoError> {
        if properties.len() > MAX_COMPILE_PROPERTIES {
            return Err(OpenVinoError::InvalidInput(format!(
                "at most {MAX_COMPILE_PROPERTIES} compile properties are supported, got {}",
                properties.len()
            )));
        }
        let c_device = CString::new(device_name).map_err(|_| {
            OpenVinoError::InvalidInput(format!("device name contains null byte: {device_name}"))
        })?;
        let mut c_props = Vec::with_capacity(properties.len());
        for (key, value) in properties {
            let c_key = CString::new(*key).map_err(|_| {
                OpenVinoError::InvalidInput(format!("property key contains null byte: {key}"))
            })?;
            let c_value = CString::new(*value).map_err(|_| {
                OpenVinoError::InvalidInput(format!("property value contains null byte: {value}"))
            })?;
            c_props.push((c_key, c_value));
        }
        let c_refs: Vec<(&CStr, &CStr)> = c_props
            .iter()
            .map(|(k, v)| (k.as_c_str(), v.as_c_str()))
            .collect();

        let mut compiled_ptr = std::ptr::null_mut();
        let status = unsafe {
            self.lib.compile_model_with_properties(
                self.ptr,
                model.as_ptr(),
                c_device.as_ptr(),
                &c_refs,
                &mut compiled_ptr,
            )
        };
        check_status(status, &self.lib).map_err(|e| match e {
            OpenVinoError::StatusError { message, .. } => OpenVinoError::ModelCompilationFailed(
                format!("Device {device_name} compilation error: {message}"),
            ),
            other => other,
        })?;

        OpenVinoCompiledModel::from_raw(compiled_ptr, self.lib.clone())
    }

    /// Access the underlying loaded library handle.
    #[must_use]
    pub fn library(&self) -> &Arc<OpenVinoLibrary> {
        &self.lib
    }
}

impl Drop for OpenVinoCore {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            unsafe {
                (self.lib.ov_core_free)(self.ptr);
            }
            self.ptr = std::ptr::null_mut();
        }
    }
}
