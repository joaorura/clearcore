use serde::{Deserialize, Serialize};

/// Target precision mode for TensorRT compilation and inference execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum PrecisionTarget {
    /// 16-bit half precision floating point (IEEE 754-2008 binary16).
    /// Supported across all modern NVIDIA architectures (Turing sm_75 through Blackwell sm_120).
    #[default]
    Fp16,
    /// 8-bit floating point precision (FP8 E4M3 / E5M2).
    /// Native 5th-generation Tensor Core acceleration on NVIDIA Blackwell (`sm_120`),
    /// Hopper (`sm_90`), and Ada Lovelace (`sm_89`).
    Fp8,
    /// 8-bit signed integer precision (requires a calibrated or QAT-quantized engine).
    /// Used only when the GPU has no FP8 tensor cores and no fast FP16 path (e.g. Pascal `sm_61`).
    Int8,
    /// 32-bit single precision floating point reference fallback.
    ///
    /// Clearcore policy: FP32 is reserved for the x86 CPU baseline. It is never chosen
    /// automatically for a GPU (see [`crate::GpuDevice::preferred_precision`]).
    Fp32,
}

impl PrecisionTarget {
    /// Returns the canonical string representation for telemetry and diagnostics.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Fp16 => "fp16",
            Self::Fp8 => "fp8",
            Self::Int8 => "int8",
            Self::Fp32 => "fp32",
        }
    }

    /// Returns true if this precision is accelerated natively by Blackwell sm_120 Tensor Cores.
    #[must_use]
    pub const fn is_sm120_accelerated(self) -> bool {
        matches!(self, Self::Fp16 | Self::Fp8 | Self::Int8)
    }

    /// Size in bytes for a single audio sample representation in this precision.
    #[must_use]
    pub const fn bytes_per_sample(self) -> usize {
        match self {
            Self::Fp32 => 4,
            Self::Fp16 => 2,
            Self::Fp8 | Self::Int8 => 1,
        }
    }

    /// Parses a string into a precision target.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().as_str() {
            "fp16" | "half" => Some(Self::Fp16),
            "fp8" => Some(Self::Fp8),
            "int8" | "i8" => Some(Self::Int8),
            "fp32" | "single" => Some(Self::Fp32),
            _ => None,
        }
    }
}
