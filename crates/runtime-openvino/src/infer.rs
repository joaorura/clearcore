//! Safe OpenVINO InferRequest abstraction.

use std::ffi::CString;
use std::sync::Arc;

use crate::error::{OpenVinoError, check_status};
use crate::ffi::{OpenVinoLibrary, ov_infer_request_t};
use crate::tensor::OpenVinoTensor;

/// Safe wrapper for an OpenVINO inference request.
pub struct OpenVinoInferRequest {
    ptr: *mut ov_infer_request_t,
    lib: Arc<OpenVinoLibrary>,
}

unsafe impl Send for OpenVinoInferRequest {}

impl OpenVinoInferRequest {
    /// Constructs a safe wrapper from a raw OpenVINO infer request pointer.
    pub fn from_raw(
        ptr: *mut ov_infer_request_t,
        lib: Arc<OpenVinoLibrary>,
    ) -> Result<Self, OpenVinoError> {
        if ptr.is_null() {
            return Err(OpenVinoError::InferRequestFailed(
                "received null infer request pointer".to_owned(),
            ));
        }
        Ok(Self { ptr, lib })
    }

    /// Executes synchronous inference on the prepared tensors.
    pub fn infer(&mut self) -> Result<(), OpenVinoError> {
        let status = unsafe { (self.lib.ov_infer_request_infer)(self.ptr) };
        check_status(status, &self.lib).map_err(|e| match e {
            OpenVinoError::StatusError { message, .. } => {
                OpenVinoError::InferenceExecutionFailed(message)
            }
            other => other,
        })
    }

    /// Retrieves an input or output tensor by name.
    pub fn get_tensor(&self, name: &str) -> Result<OpenVinoTensor, OpenVinoError> {
        let c_name = CString::new(name).map_err(|_| {
            OpenVinoError::InvalidInput(format!("tensor name contains null byte: {name}"))
        })?;

        let mut tensor_ptr = std::ptr::null_mut();
        let status = unsafe {
            (self.lib.ov_infer_request_get_tensor)(self.ptr, c_name.as_ptr(), &mut tensor_ptr)
        };
        check_status(status, &self.lib)?;
        OpenVinoTensor::from_raw(tensor_ptr, self.lib.clone())
    }

    /// Sets an input or output tensor by name.
    pub fn set_tensor(&mut self, name: &str, tensor: &OpenVinoTensor) -> Result<(), OpenVinoError> {
        let c_name = CString::new(name).map_err(|_| {
            OpenVinoError::InvalidInput(format!("tensor name contains null byte: {name}"))
        })?;

        let status = unsafe {
            (self.lib.ov_infer_request_set_tensor)(self.ptr, c_name.as_ptr(), tensor.as_ptr())
        };
        check_status(status, &self.lib)
    }

    /// Retrieves the input tensor at the given port index.
    pub fn get_input_tensor_by_index(&self, index: usize) -> Result<OpenVinoTensor, OpenVinoError> {
        let mut tensor_ptr = std::ptr::null_mut();
        let status = unsafe {
            (self.lib.ov_infer_request_get_input_tensor_by_index)(self.ptr, index, &mut tensor_ptr)
        };
        check_status(status, &self.lib)?;
        OpenVinoTensor::from_raw(tensor_ptr, self.lib.clone())
    }

    /// Retrieves the output tensor at the given port index.
    pub fn get_output_tensor_by_index(
        &self,
        index: usize,
    ) -> Result<OpenVinoTensor, OpenVinoError> {
        let mut tensor_ptr = std::ptr::null_mut();
        let status = unsafe {
            (self.lib.ov_infer_request_get_output_tensor_by_index)(self.ptr, index, &mut tensor_ptr)
        };
        check_status(status, &self.lib)?;
        OpenVinoTensor::from_raw(tensor_ptr, self.lib.clone())
    }

    /// Returns the raw pointer to the underlying OpenVINO infer request.
    #[must_use]
    pub fn as_ptr(&self) -> *const ov_infer_request_t {
        self.ptr
    }
}

impl Drop for OpenVinoInferRequest {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            unsafe {
                (self.lib.ov_infer_request_free)(self.ptr);
            }
            self.ptr = std::ptr::null_mut();
        }
    }
}
