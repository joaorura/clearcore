//! Apple Silicon CoreML Runtime for Clearcore Realtime Noise Suppression.
//!
//! Provides native hardware acceleration targeting the Apple Neural Engine (ANE)
//! and Metal GPU on macOS (Apple Silicon M-series), with a high-fidelity
//! cross-platform mock/stub execution layer for Linux and Windows testing.

pub mod error;
pub mod ffi;
pub mod runner;
pub mod tensor;
pub mod types;

pub use error::CoreMlError;
pub use runner::CoreMlModelRunner;
pub use tensor::CoreMlTensor;
pub use types::{ComputeUnit, is_apple_silicon_available, is_coreml_available};
