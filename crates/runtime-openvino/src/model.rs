//! Safe OpenVINO Model and CompiledModel abstractions.

use std::ffi::{CStr, CString};
use std::sync::Arc;

use crate::error::{OpenVinoError, check_status};
use crate::ffi::{
    OpenVinoLibrary, ov_compiled_model_t, ov_dimension_t, ov_model_t, ov_partial_shape_t,
};
use crate::infer::OpenVinoInferRequest;

/// Port name and dimensions (`None` = dynamic).
pub type PortShape = (String, Vec<Option<i64>>);

/// Safe wrapper for an uncompiled OpenVINO model (IR / ONNX representation).
pub struct OpenVinoModel {
    ptr: *mut ov_model_t,
    lib: Arc<OpenVinoLibrary>,
}

unsafe impl Send for OpenVinoModel {}
unsafe impl Sync for OpenVinoModel {}

impl OpenVinoModel {
    /// Constructs a safe wrapper from a raw OpenVINO model pointer.
    pub fn from_raw(
        ptr: *mut ov_model_t,
        lib: Arc<OpenVinoLibrary>,
    ) -> Result<Self, OpenVinoError> {
        if ptr.is_null() {
            return Err(OpenVinoError::ModelReadFailed(
                "received null model pointer".to_owned(),
            ));
        }
        Ok(Self { ptr, lib })
    }

    /// Returns the raw pointer to the underlying OpenVINO model.
    #[must_use]
    pub fn as_ptr(&self) -> *const ov_model_t {
        self.ptr
    }

    /// Names (any tensor name) of the model inputs, in port order.
    pub fn input_names(&self) -> Result<Vec<String>, OpenVinoError> {
        let mut count = 0usize;
        let status = unsafe { (self.lib.ov_model_inputs_size)(self.ptr, &raw mut count) };
        check_status(status, &self.lib)?;
        let mut names = Vec::with_capacity(count);
        for index in 0..count {
            let mut port = std::ptr::null_mut();
            let status =
                unsafe { (self.lib.ov_model_const_input_by_index)(self.ptr, index, &raw mut port) };
            check_status(status, &self.lib)?;
            if port.is_null() {
                return Err(OpenVinoError::ShapeError(format!(
                    "input port {index} is null"
                )));
            }
            let mut name_ptr: *mut std::os::raw::c_char = std::ptr::null_mut();
            let status = unsafe { (self.lib.ov_port_get_any_name)(port, &raw mut name_ptr) };
            let result = check_status(status, &self.lib).map(|()| {
                if name_ptr.is_null() {
                    String::new()
                } else {
                    // SAFETY: non-null NUL-terminated string allocated by OpenVINO.
                    unsafe { CStr::from_ptr(name_ptr) }
                        .to_string_lossy()
                        .into_owned()
                }
            });
            // SAFETY: both were allocated by OpenVINO for this call and are not used afterwards.
            unsafe {
                if !name_ptr.is_null() {
                    (self.lib.ov_free)(name_ptr);
                }
                (self.lib.ov_output_const_port_free)(port);
            }
            names.push(result?);
        }
        Ok(names)
    }

    /// Name and dimensions (`None` = dynamic) of every input (`outputs == false`) or output
    /// port, in port order.
    pub fn ports(&self, outputs: bool) -> Result<Vec<PortShape>, OpenVinoError> {
        let mut count = 0usize;
        let status = unsafe {
            if outputs {
                (self.lib.ov_model_outputs_size)(self.ptr, &raw mut count)
            } else {
                (self.lib.ov_model_inputs_size)(self.ptr, &raw mut count)
            }
        };
        check_status(status, &self.lib)?;
        let mut ports = Vec::with_capacity(count);
        for index in 0..count {
            let mut port = std::ptr::null_mut();
            let status = unsafe {
                if outputs {
                    (self.lib.ov_model_const_output_by_index)(self.ptr, index, &raw mut port)
                } else {
                    (self.lib.ov_model_const_input_by_index)(self.ptr, index, &raw mut port)
                }
            };
            check_status(status, &self.lib)?;
            if port.is_null() {
                return Err(OpenVinoError::ShapeError(format!("port {index} is null")));
            }
            let mut name_ptr: *mut std::os::raw::c_char = std::ptr::null_mut();
            let name_status = unsafe { (self.lib.ov_port_get_any_name)(port, &raw mut name_ptr) };
            let name = check_status(name_status, &self.lib).map(|()| {
                if name_ptr.is_null() {
                    String::new()
                } else {
                    // SAFETY: non-null NUL-terminated string allocated by OpenVINO.
                    unsafe { CStr::from_ptr(name_ptr) }
                        .to_string_lossy()
                        .into_owned()
                }
            });
            let mut partial = ov_partial_shape_t {
                rank: ov_dimension_t { min: 0, max: 0 },
                dims: std::ptr::null_mut(),
            };
            let shape_status =
                unsafe { (self.lib.ov_port_get_partial_shape)(port, &raw mut partial) };
            let dims = check_status(shape_status, &self.lib).and_then(|()| {
                if partial.rank.min != partial.rank.max || partial.rank.min < 0 {
                    return Err(OpenVinoError::ShapeError(format!(
                        "port {index} has a dynamic rank"
                    )));
                }
                let rank = usize::try_from(partial.rank.min)
                    .map_err(|_| OpenVinoError::ShapeError("rank overflows".to_owned()))?;
                let mut dims = Vec::with_capacity(rank);
                for d in 0..rank {
                    // SAFETY: `dims` holds `rank` entries allocated by OpenVINO.
                    let dim = unsafe { *partial.dims.add(d) };
                    dims.push((dim.min == dim.max && dim.min >= 0).then_some(dim.min));
                }
                Ok(dims)
            });
            // SAFETY: allocated by OpenVINO for this call and not used afterwards.
            unsafe {
                if !name_ptr.is_null() {
                    (self.lib.ov_free)(name_ptr);
                }
                if shape_status == crate::ffi::ov_status_e::OK {
                    (self.lib.ov_partial_shape_free)(&raw mut partial);
                }
                (self.lib.ov_output_const_port_free)(port);
            }
            ports.push((name?, dims?));
        }
        Ok(ports)
    }

    /// Whether the model declares an input with this tensor name.
    pub fn has_input(&self, tensor_name: &str) -> Result<bool, OpenVinoError> {
        Ok(self.input_names()?.iter().any(|name| name == tensor_name))
    }

    /// Pins one input (by tensor name) to a fully static shape.
    ///
    /// Required before compiling for devices that reject dynamic shapes (Intel NPU), and it
    /// lets every device specialise its kernels for the real-time `S = 1` hop.
    pub fn reshape_input(&mut self, tensor_name: &str, shape: &[i64]) -> Result<(), OpenVinoError> {
        let c_name = CString::new(tensor_name).map_err(|_| {
            OpenVinoError::InvalidInput(format!("tensor name contains null byte: {tensor_name}"))
        })?;
        let rank = i64::try_from(shape.len())
            .map_err(|_| OpenVinoError::ShapeError("shape rank overflows i64".to_owned()))?;
        let mut partial = ov_partial_shape_t {
            rank: ov_dimension_t { min: 0, max: 0 },
            dims: std::ptr::null_mut(),
        };
        let status = unsafe {
            (self.lib.ov_partial_shape_create_static)(rank, shape.as_ptr(), &mut partial)
        };
        check_status(status, &self.lib)?;
        // The partial shape is passed by value; it still owns `dims`, so free it afterwards.
        let status = unsafe {
            (self.lib.ov_model_reshape_input_by_name)(self.ptr, c_name.as_ptr(), partial)
        };
        let result = check_status(status, &self.lib).map_err(|e| match e {
            OpenVinoError::StatusError { message, .. } => OpenVinoError::ShapeError(format!(
                "reshape of input '{tensor_name}' to {shape:?} failed: {message}"
            )),
            other => other,
        });
        unsafe {
            (self.lib.ov_partial_shape_free)(&mut partial);
        }
        result
    }
}

impl Drop for OpenVinoModel {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            unsafe {
                (self.lib.ov_model_free)(self.ptr);
            }
            self.ptr = std::ptr::null_mut();
        }
    }
}

/// Safe wrapper for an OpenVINO compiled model targeted to a specific device.
pub struct OpenVinoCompiledModel {
    ptr: *mut ov_compiled_model_t,
    lib: Arc<OpenVinoLibrary>,
}

unsafe impl Send for OpenVinoCompiledModel {}
unsafe impl Sync for OpenVinoCompiledModel {}

impl OpenVinoCompiledModel {
    /// Constructs a safe wrapper from a raw compiled model pointer.
    pub fn from_raw(
        ptr: *mut ov_compiled_model_t,
        lib: Arc<OpenVinoLibrary>,
    ) -> Result<Self, OpenVinoError> {
        if ptr.is_null() {
            return Err(OpenVinoError::ModelCompilationFailed(
                "received null compiled model pointer".to_owned(),
            ));
        }
        Ok(Self { ptr, lib })
    }

    /// Instantiates an inference request for this compiled model.
    pub fn create_infer_request(&self) -> Result<OpenVinoInferRequest, OpenVinoError> {
        let mut req_ptr = std::ptr::null_mut();
        let status =
            unsafe { (self.lib.ov_compiled_model_create_infer_request)(self.ptr, &mut req_ptr) };
        check_status(status, &self.lib).map_err(|e| match e {
            OpenVinoError::StatusError { message, .. } => {
                OpenVinoError::InferRequestFailed(message)
            }
            other => other,
        })?;
        OpenVinoInferRequest::from_raw(req_ptr, self.lib.clone())
    }

    /// Returns the raw pointer to the underlying compiled model.
    #[must_use]
    pub fn as_ptr(&self) -> *const ov_compiled_model_t {
        self.ptr
    }
}

impl Drop for OpenVinoCompiledModel {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            unsafe {
                (self.lib.ov_compiled_model_free)(self.ptr);
            }
            self.ptr = std::ptr::null_mut();
        }
    }
}
