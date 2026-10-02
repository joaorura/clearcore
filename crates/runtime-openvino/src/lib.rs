//! Intel OpenVINO Realtime Runtime for Clearcore.
//!
//! Provides a safe, idiomatic Rust API over Intel's OpenVINO C library (`libopenvino_c.so.2510`),
//! supporting accelerated neural inference on Intel NPU (Arrow Lake / Meteor Lake / Lunar Lake),
//! Intel Arc & integrated GPUs (OpenCL/Level-Zero), and Intel CPUs (AVX2/AVX-512/AMX/VNNI).
//!
//! All `unsafe` code is strictly isolated inside this crate.

#![allow(unsafe_code)]
#![allow(
    clippy::module_name_repetitions,
    clippy::missing_errors_doc,
    clippy::must_use_candidate
)]

pub mod core;
pub mod dfn3;
pub mod error;
pub mod ffi;
pub mod infer;
pub mod model;
pub mod precision;
pub mod tensor;

pub use core::OpenVinoCore;
pub use dfn3::{Dfn3Graphs, Dfn3Output, Dfn3Session};
pub use error::OpenVinoError;
pub use ffi::{OpenVinoLibrary, locate_library, ov_element_type_e, ov_status_e};
pub use infer::OpenVinoInferRequest;
pub use model::{OpenVinoCompiledModel, OpenVinoModel};
pub use precision::{DeviceKind, InferencePrecision, fallback_order, precision_for_device};
pub use tensor::OpenVinoTensor;

/// Checks if the OpenVINO dynamic runtime library is installed and loadable on the host.
#[must_use]
pub fn is_openvino_available() -> bool {
    OpenVinoLibrary::get_or_init().is_ok()
}

/// Returns the detected hardware devices supported by OpenVINO on this system.
/// If OpenVINO is not installed, returns an empty list.
#[must_use]
pub fn available_devices() -> Vec<String> {
    OpenVinoCore::new()
        .and_then(|core| core.available_devices())
        .unwrap_or_default()
}

/// Returns the OpenVINO runtime version string if available.
#[must_use]
pub fn openvino_version() -> Option<String> {
    OpenVinoCore::new().and_then(|core| core.version()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_library_discovery_and_version() {
        if !is_openvino_available() {
            eprintln!("OpenVINO library not found on this system; skipping test");
            return;
        }

        let core = OpenVinoCore::new().expect("OpenVinoCore initialization failed");
        let version = core.version().expect("Querying version failed");
        assert!(!version.is_empty(), "version should not be empty");
        assert!(
            version.contains("2025") || version.contains("OpenVINO"),
            "expected version 2025.x or OpenVINO, got: {version}"
        );

        let devices = core
            .available_devices()
            .expect("Querying available devices failed");
        assert!(!devices.is_empty(), "devices list should not be empty");
        assert!(
            devices.iter().any(|d| d.eq_ignore_ascii_case("CPU")),
            "CPU device should always be supported by OpenVINO"
        );
    }

    #[test]
    fn test_tensor_allocation_and_read_write() {
        if !is_openvino_available() {
            return;
        }

        let lib = OpenVinoLibrary::get_or_init().expect("load lib failed");
        let shape = [1, 2, 480];
        let num_elements = (2 * 480) as usize;
        let mut test_data = Vec::with_capacity(num_elements);
        for i in 0..num_elements {
            test_data.push(i as f32 * 0.01);
        }

        let mut tensor =
            OpenVinoTensor::from_slice_f32(&shape, &test_data, lib).expect("create tensor");
        assert_eq!(tensor.shape().expect("shape"), vec![1, 2, 480]);
        assert_eq!(
            tensor.byte_size().expect("byte_size"),
            num_elements * std::mem::size_of::<f32>()
        );

        let slice = tensor.as_slice_f32().expect("slice");
        assert_eq!(slice.len(), num_elements);
        assert!((slice[0] - test_data[0]).abs() < 1e-6);
        assert!((slice[10] - test_data[10]).abs() < 1e-6);

        // Mutate slice
        let mut_slice = tensor.as_mut_slice_f32().expect("mut slice");
        mut_slice[0] = 42.0;
        let slice_after = tensor.as_slice_f32().expect("slice after");
        assert!((slice_after[0] - 42.0).abs() < 1e-6);
    }

    #[test]
    fn test_model_read_nonexistent_fails_gracefully() {
        if !is_openvino_available() {
            return;
        }

        let core = OpenVinoCore::new().expect("OpenVinoCore init");
        let result = core.read_model_from_file("/tmp/nonexistent_model.xml", None::<&str>);
        assert!(result.is_err(), "reading nonexistent model must return Err");
        let err = result.err().unwrap();
        assert!(
            matches!(
                err,
                OpenVinoError::ModelReadFailed(_) | OpenVinoError::StatusError { .. }
            ),
            "expected ModelReadFailed or StatusError, got: {err:?}"
        );
    }

    #[test]
    fn test_onnx_model_read_and_cpu_compilation_if_file_exists() {
        if !is_openvino_available() {
            return;
        }

        let onnx_path = "models/stateful/enc.onnx";
        if !std::path::Path::new(onnx_path).exists() {
            return;
        }

        let core = OpenVinoCore::new().expect("core init");
        let model = core
            .read_model_from_file(onnx_path, None::<&str>)
            .expect("read onnx model");

        // Compile for CPU (CPU plugin handles dynamic shapes properly)
        let compiled = core
            .compile_model(&model, "CPU")
            .expect("compile model for CPU");
        let infer_req = compiled.create_infer_request();
        assert!(infer_req.is_ok(), "create infer request should succeed");
    }
}
