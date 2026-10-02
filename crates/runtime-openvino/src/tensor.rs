//! Safe OpenVINO Tensor abstraction.

use std::sync::Arc;

use crate::error::{OpenVinoError, check_status};
use crate::ffi::{OpenVinoLibrary, ov_element_type_e, ov_shape_t, ov_tensor_t};

/// Safe wrapper around an OpenVINO native tensor.
pub struct OpenVinoTensor {
    ptr: *mut ov_tensor_t,
    lib: Arc<OpenVinoLibrary>,
}

unsafe impl Send for OpenVinoTensor {}

impl OpenVinoTensor {
    /// Constructs a safe wrapper from a raw OpenVINO tensor pointer.
    pub fn from_raw(
        ptr: *mut ov_tensor_t,
        lib: Arc<OpenVinoLibrary>,
    ) -> Result<Self, OpenVinoError> {
        if ptr.is_null() {
            return Err(OpenVinoError::TensorCreationFailed(
                "received null tensor pointer".to_owned(),
            ));
        }
        Ok(Self { ptr, lib })
    }

    /// Allocates an OpenVINO tensor with the given shape and `F32` element type.
    pub fn create_f32(shape: &[i64], lib: Arc<OpenVinoLibrary>) -> Result<Self, OpenVinoError> {
        let mut ov_shape = ov_shape_t {
            rank: 0,
            dims: std::ptr::null_mut(),
        };

        let status =
            unsafe { (lib.ov_shape_create)(shape.len() as i64, shape.as_ptr(), &mut ov_shape) };
        check_status(status, &lib)?;

        let mut tensor_ptr = std::ptr::null_mut();
        let status =
            unsafe { (lib.ov_tensor_create)(ov_element_type_e::F32, ov_shape, &mut tensor_ptr) };

        // Free the temporary shape struct memory allocated by OpenVINO
        unsafe {
            let _ = (lib.ov_shape_free)(&mut ov_shape);
        }

        check_status(status, &lib)?;
        Self::from_raw(tensor_ptr, lib)
    }

    /// Creates an OpenVINO tensor and copies the provided slice data into it.
    pub fn from_slice_f32(
        shape: &[i64],
        data: &[f32],
        lib: Arc<OpenVinoLibrary>,
    ) -> Result<Self, OpenVinoError> {
        let mut tensor = Self::create_f32(shape, lib)?;
        let tensor_slice = tensor.as_mut_slice_f32()?;
        if tensor_slice.len() != data.len() {
            return Err(OpenVinoError::InvalidInput(format!(
                "data length ({}) does not match tensor capacity ({})",
                data.len(),
                tensor_slice.len()
            )));
        }
        tensor_slice.copy_from_slice(data);
        Ok(tensor)
    }

    /// Retrieves the shape dimensions of this tensor.
    pub fn shape(&self) -> Result<Vec<i64>, OpenVinoError> {
        let mut ov_shape = ov_shape_t {
            rank: 0,
            dims: std::ptr::null_mut(),
        };

        let status = unsafe { (self.lib.ov_tensor_get_shape)(self.ptr, &mut ov_shape) };
        check_status(status, &self.lib)?;

        if ov_shape.rank <= 0 || ov_shape.dims.is_null() {
            unsafe {
                let _ = (self.lib.ov_shape_free)(&mut ov_shape);
            }
            return Ok(Vec::new());
        }

        let dims_slice =
            unsafe { std::slice::from_raw_parts(ov_shape.dims, ov_shape.rank as usize) };
        let dims = dims_slice.to_vec();

        unsafe {
            let _ = (self.lib.ov_shape_free)(&mut ov_shape);
        }

        Ok(dims)
    }

    /// Retrieves the total byte size of the tensor memory buffer.
    pub fn byte_size(&self) -> Result<usize, OpenVinoError> {
        let mut size = 0usize;
        let status = unsafe { (self.lib.ov_tensor_get_byte_size)(self.ptr, &mut size) };
        check_status(status, &self.lib)?;
        Ok(size)
    }

    /// Borrows the tensor buffer as a slice of 32-bit floats.
    pub fn as_slice_f32(&self) -> Result<&[f32], OpenVinoError> {
        let bytes = self.byte_size()?;
        let num_elements = bytes / std::mem::size_of::<f32>();

        let mut data_ptr = std::ptr::null_mut();
        let status = unsafe { (self.lib.ov_tensor_data)(self.ptr, &mut data_ptr) };
        check_status(status, &self.lib)?;

        if data_ptr.is_null() {
            return Err(OpenVinoError::TensorAccessFailed(
                "data pointer is null".to_owned(),
            ));
        }

        let slice = unsafe { std::slice::from_raw_parts(data_ptr.cast::<f32>(), num_elements) };
        Ok(slice)
    }

    /// Mutably borrows the tensor buffer as a slice of 32-bit floats.
    pub fn as_mut_slice_f32(&mut self) -> Result<&mut [f32], OpenVinoError> {
        let bytes = self.byte_size()?;
        let num_elements = bytes / std::mem::size_of::<f32>();

        let mut data_ptr = std::ptr::null_mut();
        let status = unsafe { (self.lib.ov_tensor_data)(self.ptr, &mut data_ptr) };
        check_status(status, &self.lib)?;

        if data_ptr.is_null() {
            return Err(OpenVinoError::TensorAccessFailed(
                "data pointer is null".to_owned(),
            ));
        }

        let slice = unsafe { std::slice::from_raw_parts_mut(data_ptr.cast::<f32>(), num_elements) };
        Ok(slice)
    }

    /// Returns the raw pointer to the underlying OpenVINO tensor.
    #[must_use]
    pub fn as_ptr(&self) -> *const ov_tensor_t {
        self.ptr
    }

    /// Returns the raw mutable pointer to the underlying OpenVINO tensor.
    #[must_use]
    pub fn as_mut_ptr(&mut self) -> *mut ov_tensor_t {
        self.ptr
    }
}

impl Drop for OpenVinoTensor {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            unsafe {
                (self.lib.ov_tensor_free)(self.ptr);
            }
            self.ptr = std::ptr::null_mut();
        }
    }
}
