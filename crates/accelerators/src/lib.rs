#![forbid(unsafe_code)]

pub mod auto;
pub mod coreml;
pub mod directml;
pub mod hardware;
pub mod openvino;
pub mod ryzenai;
pub mod tensorrt;
pub mod vulkan;

pub use auto::{
    AutoPolicy, BackendRequest, BackendSelection, CalibrationReport, DeviceTier, PromotionDecision,
    QUALIFICATION_MAX_DEADLINE_MS, QUALIFICATION_MAX_P99_MS, backend_priority_score,
    backend_selection_from_name, evaluate_calibration, select_auto, select_best,
};
pub use coreml::CoreMlBackend;
pub use directml::DirectMlBackend;
pub use hardware::{
    DetectedHardware, HardwareAudit, HardwareScanner, MAX_RUNTIME_INSTALL_PROMPTS,
    RuntimeRecommendationTracker, RuntimeStatus,
};
pub use openvino::{APPROVED_STATEFUL_DIGESTS, OpenVINOBackend, StatefulDigests};
pub use ryzenai::RyzenAiBackend;
pub use tensorrt::TensorRtBackend;
pub use vulkan::VulkanBackend;
