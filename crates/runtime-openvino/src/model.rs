//! Safe OpenVINO Model and CompiledModel abstractions.

use std::ffi::CString;
use std::sync::Arc;

use crate::error::{OpenVinoError, check_status};
use crate::ffi::{
    OpenVinoLibrary, ov_compiled_model_t, ov_dimension_t, ov_model_t, ov_partial_shape_t,
};
use crate::infer::OpenVinoInferRequest;

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
