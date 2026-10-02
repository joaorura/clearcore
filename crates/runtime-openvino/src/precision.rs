//! Clearcore precision policy for OpenVINO devices.
//!
//! Policy (decided by the project owner): FP8 where the hardware supports it, otherwise INT8 or
//! FP16; FP32 only on the x86 CPU baseline.
//!
//! What this means for OpenVINO today:
//! - Intel NPU and Intel GPU (Arc / iGPU) have no FP8 compute path in the OpenVINO plugins, and
//!   INT8 needs a graph quantized offline (NNCF); the stateful DeepFilterNet3 assets are FP32
//!   weights, so the policy resolves to **FP16** through `INFERENCE_PRECISION_HINT=f16`.
//! - The x86 CPU plugin runs the model in **FP32** (`INFERENCE_PRECISION_HINT=f32`).
//! - Non-x86 CPUs are not the FP32 baseline, so they resolve to FP16.

/// Compute precision requested from an OpenVINO plugin through `INFERENCE_PRECISION_HINT`.
///
/// Model inputs and outputs stay `f32`; the hint only controls the internal compute precision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InferencePrecision {
    Fp16,
    Fp32,
}

impl InferencePrecision {
    /// Value of the `INFERENCE_PRECISION_HINT` property.
    #[must_use]
    pub const fn hint(self) -> &'static str {
        match self {
            Self::Fp16 => "f16",
            Self::Fp32 => "f32",
        }
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Fp16 => "fp16",
            Self::Fp32 => "fp32",
        }
    }
}

/// Category of an OpenVINO device name (`NPU`, `GPU`, `GPU.1`, `CPU`, ...).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DeviceKind {
    Npu,
    Gpu,
    Cpu,
    Other,
}

impl DeviceKind {
    /// Classifies an OpenVINO device name, ignoring case and the `.index` suffix.
    #[must_use]
    pub fn from_device_name(name: &str) -> Self {
        let base = name.split('.').next().unwrap_or(name);
        if base.eq_ignore_ascii_case("NPU") {
            Self::Npu
        } else if base.eq_ignore_ascii_case("GPU") {
            Self::Gpu
        } else if base.eq_ignore_ascii_case("CPU") {
            Self::Cpu
        } else {
            Self::Other
        }
    }

    /// Order in which `auto` tries devices: NPU, then GPU, then CPU (the project's tier order).
    #[must_use]
    pub const fn priority(self) -> u8 {
        match self {
            Self::Npu => 0,
            Self::Gpu => 1,
            Self::Cpu => 2,
            Self::Other => 3,
        }
    }
}

/// Applies the Clearcore precision policy to a device name.
#[must_use]
pub fn precision_for_device(device_name: &str) -> InferencePrecision {
    match DeviceKind::from_device_name(device_name) {
        DeviceKind::Cpu if cfg!(target_arch = "x86_64") => InferencePrecision::Fp32,
        _ => InferencePrecision::Fp16,
    }
}

/// Sorts the devices reported by OpenVINO into fallback order (NPU, GPU, CPU), dropping the
/// ones this crate does not drive (virtual devices such as `AUTO`, `HETERO`, `BATCH`).
#[must_use]
pub fn fallback_order(available: &[String]) -> Vec<String> {
    let mut devices: Vec<&String> = available
        .iter()
        .filter(|name| DeviceKind::from_device_name(name) != DeviceKind::Other)
        .collect();
    devices.sort_by_key(|name| DeviceKind::from_device_name(name).priority());
    devices.into_iter().cloned().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_is_fp16_on_accelerators_and_fp32_on_x86_cpu() {
        assert_eq!(precision_for_device("NPU"), InferencePrecision::Fp16);
        assert_eq!(precision_for_device("GPU"), InferencePrecision::Fp16);
        assert_eq!(precision_for_device("gpu.1"), InferencePrecision::Fp16);
        if cfg!(target_arch = "x86_64") {
            assert_eq!(precision_for_device("CPU"), InferencePrecision::Fp32);
        } else {
            assert_eq!(precision_for_device("CPU"), InferencePrecision::Fp16);
        }
        assert_eq!(InferencePrecision::Fp16.hint(), "f16");
        assert_eq!(InferencePrecision::Fp32.hint(), "f32");
    }

    #[test]
    fn fallback_order_is_npu_gpu_cpu_and_skips_virtual_devices() {
        let available = ["CPU", "GPU.0", "AUTO", "NPU", "GPU.1", "HETERO"]
            .map(String::from)
            .to_vec();
        assert_eq!(
            fallback_order(&available),
            ["NPU", "GPU.0", "GPU.1", "CPU"].map(String::from).to_vec()
        );
        assert!(fallback_order(&[]).is_empty());
    }
}
