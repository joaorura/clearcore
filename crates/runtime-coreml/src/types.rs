use serde::{Deserialize, Serialize};

/// Target compute unit for CoreML model execution.
///
/// Directs CoreML to dispatch inference across the Apple Neural Engine (ANE),
/// Metal GPU, or CPU.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum ComputeUnit {
    /// Adaptive dispatch across Apple Neural Engine (ANE), Metal GPU, and CPU.
    /// Maps to CoreML `MLComputeUnitsAll`.
    #[default]
    All,
    /// Restrict inference to Apple Neural Engine and CPU (bypassing Metal GPU).
    /// Maps to CoreML `MLComputeUnitsCPUAndNeuralEngine`.
    CpuAndNeuralEngine,
    /// Restrict inference to Metal GPU and CPU (bypassing Apple Neural Engine).
    /// Maps to CoreML `MLComputeUnitsCPUAndGPU`.
    CpuAndGpu,
    /// Execute solely on the CPU.
    /// Maps to CoreML `MLComputeUnitsCPUOnly`.
    CpuOnly,
}

impl ComputeUnit {
    /// Parses a compute unit name from a string.
    #[must_use]
    pub fn from_str_name(name: &str) -> Self {
        match name.to_ascii_lowercase().as_str() {
            "all" | "ane_and_gpu" | "auto" | "default" => Self::All,
            "ane" | "npu" | "neural_engine" | "cpu_and_ane" => Self::CpuAndNeuralEngine,
            "gpu" | "metal" | "cpu_and_gpu" => Self::CpuAndGpu,
            "cpu" | "cpu_only" => Self::CpuOnly,
            _ => Self::All,
        }
    }

    /// Canonical string identifier for this compute unit.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::CpuAndNeuralEngine => "ane",
            Self::CpuAndGpu => "gpu",
            Self::CpuOnly => "cpu",
        }
    }

    /// Converts to the native CoreML integer representation (`MLComputeUnits`).
    #[must_use]
    pub const fn to_ml_compute_units(self) -> i64 {
        match self {
            Self::CpuOnly => 0,            // MLComputeUnitsCPUOnly
            Self::CpuAndGpu => 1,          // MLComputeUnitsCPUAndGPU
            Self::All => 2,                // MLComputeUnitsAll
            Self::CpuAndNeuralEngine => 3, // MLComputeUnitsCPUAndNeuralEngine
        }
    }

    /// Indicates whether the Apple Neural Engine (ANE) is eligible for execution.
    #[must_use]
    pub const fn is_ane_enabled(self) -> bool {
        matches!(self, Self::All | Self::CpuAndNeuralEngine)
    }

    /// Indicates whether the Metal GPU is eligible for execution.
    #[must_use]
    pub const fn is_gpu_enabled(self) -> bool {
        matches!(self, Self::All | Self::CpuAndGpu)
    }
}

/// Probes whether the current host platform has native Apple Silicon (M-series) hardware.
#[must_use]
pub fn is_apple_silicon_available() -> bool {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        true
    }
    #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
    {
        false
    }
}

/// Probes whether the CoreML runtime framework is available on the host operating system.
#[must_use]
pub fn is_coreml_available() -> bool {
    #[cfg(target_os = "macos")]
    {
        true
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}
