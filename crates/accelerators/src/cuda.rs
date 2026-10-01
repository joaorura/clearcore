#![forbid(unsafe_code)]
#![allow(clippy::doc_markdown)]

//! Legacy CUDA module alias for NVIDIA acceleration.
//! In accordance with the system design ("sem CUDA, só `TensorRT`"),
//! all NVIDIA acceleration is handled strictly through [`TensorRtBackend`].

pub use crate::tensorrt::TensorRtBackend;

/// Backward-compatible alias for [`TensorRtBackend`].
pub type CudaBackend = TensorRtBackend;
