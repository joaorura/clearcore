//! Universal GPU Compute & AMD NPU runtime abstractions.
//!
//! Provides hardware-level compute runtimes for:
//! - DirectML (DirectX 12 Compute on Windows)
//! - Vulkan Compute (SPIR-V compute shaders on Linux and Windows)
//! - AMD Ryzen AI NPU (Linux `amdnpu` / `amdxdna` and Windows Vitis AI)
//!
//! All FFI and unsafe system interactions are isolated within this crate,
//! exposing safe, RAII-governed Rust interfaces for inference execution.

pub mod directml;
pub mod ryzenai;
pub mod vulkan;

pub use directml::{DirectMlContext, DirectMlDeviceType, DirectMlError};
pub use ryzenai::{RyzenAiContext, RyzenAiDeviceType, RyzenAiDriverKind, RyzenAiError};
pub use vulkan::{VulkanContext, VulkanDeviceType, VulkanError};
