#![forbid(unsafe_code)]

pub mod auto;
pub mod coreml;
pub mod cuda;
pub mod openvino;
pub mod ryzenai;

pub use auto::{
    AutoPolicy, BackendRequest, BackendSelection, CalibrationReport, DeviceTier, PromotionDecision,
    QUALIFICATION_MAX_DEADLINE_MS, QUALIFICATION_MAX_P99_MS, backend_priority_score,
    backend_selection_from_name, evaluate_calibration, select_auto, select_best,
};
pub use coreml::CoreMlBackend;
pub use cuda::CudaBackend;
pub use openvino::OpenVINOBackend;
pub use ryzenai::RyzenAiBackend;
