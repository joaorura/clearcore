#![forbid(unsafe_code)]
#![allow(clippy::missing_errors_doc, clippy::missing_const_for_fn)]

pub mod backend;
pub mod backoff;
pub mod supervisor;

pub use backend::{
    BackendResolutionInfo, find_repo_root, find_stateful_model_dir,
    instantiate_backend_with_fallback,
};
pub use backoff::{BACKOFF_SECONDS, BackoffTracker, FIFTEEN_MINUTES, MAX_CRASHES_PER_15_MINUTES};
pub use supervisor::{
    EngineSupervisor, SupervisorError, SupervisorState, SupervisorStatus,
    convert_dsp_preset_to_ipc, convert_engine_mode_to_ipc, convert_ipc_mode_to_engine,
    convert_ipc_preset_to_dsp,
};
