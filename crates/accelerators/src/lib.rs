#![forbid(unsafe_code)]

pub mod auto;
pub mod coreml;
pub mod cuda;
pub mod openvino;

pub use auto::{
    AutoPolicy, BackendRequest, BackendSelection, CalibrationReport, PromotionDecision,
    QUALIFICATION_MAX_DEADLINE_MS, QUALIFICATION_MAX_P99_MS, evaluate_calibration, select_auto,
};
pub use coreml::CoreMlBackend;
pub use cuda::CudaBackend;
pub use openvino::OpenVINOBackend;
