#![forbid(unsafe_code)]
#![allow(clippy::missing_errors_doc, clippy::missing_const_for_fn)]

pub mod backend;
pub mod backoff;
pub mod supervisor;

pub use backend::{
    AUTO_QUALIFICATION_MAX_HOP_MS, BackendResolutionInfo, find_repo_root, find_stateful_model_dir,
    instantiate_backend_with_fallback, is_explicit_accelerator_request, qualify_backend_runtime,
};
pub use backoff::{BACKOFF_SECONDS, BackoffTracker, FIFTEEN_MINUTES, MAX_CRASHES_PER_15_MINUTES};
pub use supervisor::{
    DEV_BASE_MODEL_BASE, DEV_BASE_MODEL_PDFNET3, EngineSupervisor, SupervisorError,
    SupervisorState, SupervisorStatus, convert_dsp_preset_to_ipc, convert_engine_mode_to_ipc,
    convert_ipc_mode_to_engine, convert_ipc_preset_to_dsp,
};
