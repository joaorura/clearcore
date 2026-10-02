pub mod bindings;
pub mod context;
pub mod runtime;

use crate::precision::PrecisionTarget;

pub use bindings::{TensorRtLibrary, is_tensorrt_available, locate_library};
pub use context::{DFN3_GRU_STATE_DIM, TensorRtInferenceContext};
pub use runtime::TensorRtRuntime;

/// Primary alias for [`TensorRtInferenceContext`].
pub type TensorRtContext = TensorRtInferenceContext;

/// Execution context alias for [`TensorRtInferenceContext`].
pub type TensorRtExecutionContext = TensorRtInferenceContext;

/// Model engine representation for serialized DeepFilterNet3 TensorRT plans.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TensorRtEngine {
    pub asset_id: String,
    pub asset_sha256: String,
    pub precision: PrecisionTarget,
    pub target_sm: String,
}

impl TensorRtEngine {
    pub fn new(
        asset_id: impl Into<String>,
        asset_sha256: impl Into<String>,
        precision: PrecisionTarget,
    ) -> Self {
        Self {
            asset_id: asset_id.into(),
            asset_sha256: asset_sha256.into(),
            precision,
            target_sm: "sm_120".to_owned(),
        }
    }
}
