//! Typed errors for OpenVINO runtime operations.

use std::ffi::CStr;
use std::fmt;

use crate::ffi::{OpenVinoLibrary, ov_status_e};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenVinoError {
    LibraryNotFound(String),
    /// The loaded library is not a release this crate's FFI layout was written against.
    UnsupportedVersion(String),
    SymbolNotFound(String),
    CoreCreationFailed(String),
    ModelReadFailed(String),
    ModelCompilationFailed(String),
    InferRequestFailed(String),
    InferenceExecutionFailed(String),
    TensorCreationFailed(String),
    TensorAccessFailed(String),
    ShapeError(String),
    DeviceNotFound(String),
    InvalidInput(String),
    StatusError {
        status: i32,
        message: String,
    },
}

impl std::error::Error for OpenVinoError {}

impl fmt::Display for OpenVinoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LibraryNotFound(msg) => write!(f, "OpenVINO library not found: {msg}"),
            Self::UnsupportedVersion(msg) => write!(f, "unsupported OpenVINO version: {msg}"),
            Self::SymbolNotFound(msg) => write!(f, "OpenVINO symbol not found: {msg}"),
            Self::CoreCreationFailed(msg) => write!(f, "Failed to create OpenVINO core: {msg}"),
            Self::ModelReadFailed(msg) => write!(f, "Failed to read OpenVINO model: {msg}"),
            Self::ModelCompilationFailed(msg) => {
                write!(f, "Failed to compile OpenVINO model: {msg}")
            }
            Self::InferRequestFailed(msg) => {
                write!(f, "Failed to create OpenVINO infer request: {msg}")
            }
            Self::InferenceExecutionFailed(msg) => {
                write!(f, "OpenVINO inference execution failed: {msg}")
            }
            Self::TensorCreationFailed(msg) => {
                write!(f, "Failed to create OpenVINO tensor: {msg}")
            }
            Self::TensorAccessFailed(msg) => {
                write!(f, "Failed to access OpenVINO tensor data: {msg}")
            }
            Self::ShapeError(msg) => write!(f, "OpenVINO shape error: {msg}"),
            Self::DeviceNotFound(msg) => write!(f, "OpenVINO device not found: {msg}"),
            Self::InvalidInput(msg) => write!(f, "OpenVINO invalid input: {msg}"),
            Self::StatusError { status, message } => {
                write!(f, "OpenVINO status error ({status}): {message}")
            }
        }
    }
}

/// Validates an OpenVINO status return code and converts failures into `OpenVinoError::StatusError`.
pub fn check_status(status: ov_status_e, lib: &OpenVinoLibrary) -> Result<(), OpenVinoError> {
    if status == ov_status_e::OK {
        return Ok(());
    }

    let mut message = String::new();

    // Try last error message
    let last_err_ptr = unsafe { (lib.ov_get_last_err_msg)() };
    if !last_err_ptr.is_null() {
        let msg = unsafe { CStr::from_ptr(last_err_ptr) }.to_string_lossy();
        if !msg.is_empty() {
            message = msg.into_owned();
        }
    }

    // Fallback to error info for the status enum if last message was empty
    if message.is_empty() {
        let err_info_ptr = unsafe { (lib.ov_get_error_info)(status) };
        if !err_info_ptr.is_null() {
            message = unsafe { CStr::from_ptr(err_info_ptr) }
                .to_string_lossy()
                .into_owned();
        }
    }

    if message.is_empty() {
        message = format!("Status code: {:?}", status);
    }

    Err(OpenVinoError::StatusError {
        status: status as i32,
        message,
    })
}
