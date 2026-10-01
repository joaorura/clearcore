#![forbid(unsafe_code)]
#![allow(
    clippy::doc_markdown,
    clippy::option_if_let_else,
    clippy::collapsible_if
)]

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// Maximum number of times the system will recommend installing a missing runtime
/// for detected hardware before silently continuing with the next available backend.
pub const MAX_RUNTIME_INSTALL_PROMPTS: u32 = 3;

/// Detected physical hardware accelerator present on the host system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DetectedHardware {
    /// Physical NVIDIA Dedicated GPU (detected via PCI vendor 0x10de or /dev/nvidia*).
    NvidiaGpu,
    /// Physical Intel NPU (detected via PCI 0x8086:0x7d1d/0x7d1e or /dev/accel/*).
    IntelNpu,
    /// Physical AMD NPU (detected via PCI 0x1022:0x1502 or /dev/amdxdna).
    AmdNpu,
    /// Physical Intel Integrated or Arc GPU (detected via PCI vendor 0x8086 display class).
    IntelGpu,
    /// Physical AMD Radeon Integrated or Dedicated GPU (detected via PCI vendor 0x1002 display class).
    AmdGpu,
    /// Apple Silicon (M-series ANE / GPU).
    AppleSilicon,
    /// Standard x86_64 or ARM CPU.
    GenericCpu,
}

impl DetectedHardware {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::NvidiaGpu => "NVIDIA Dedicated GPU",
            Self::IntelNpu => "Intel NPU (Core Ultra / Arrow Lake)",
            Self::AmdNpu => "AMD Ryzen AI NPU (XDNA / XDNA 2)",
            Self::IntelGpu => "Intel Arc / iGPU",
            Self::AmdGpu => "AMD Radeon GPU / iGPU",
            Self::AppleSilicon => "Apple Silicon (ANE / GPU)",
            Self::GenericCpu => "CPU",
        }
    }

    #[must_use]
    pub const fn recommended_runtime(self) -> &'static str {
        match self {
            Self::NvidiaGpu => "TensorRT",
            Self::IntelNpu | Self::IntelGpu => "OpenVINO",
            Self::AmdNpu => "Ryzen AI Software (Vitis-AI / XRT)",
            Self::AmdGpu => "DirectML / Vulkan",
            Self::AppleSilicon => "CoreML",
            Self::GenericCpu => "Tract",
        }
    }

    #[must_use]
    pub const fn install_script(self) -> &'static str {
        match self {
            Self::NvidiaGpu => "scripts/install-tensorrt.sh",
            Self::IntelNpu | Self::IntelGpu => "scripts/install-openvino.sh",
            Self::AmdNpu => "scripts/install-ryzenai.sh",
            Self::AmdGpu => "scripts/detect-hardware.sh",
            Self::AppleSilicon | Self::GenericCpu => "",
        }
    }
}

/// Installation status of the hardware accelerator runtime.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuntimeStatus {
    /// Runtime library is installed and ready for inference.
    Installed {
        runtime_name: String,
        path: Option<String>,
    },
    /// Physical hardware was detected, but the vendor runtime is missing.
    Missing {
        recommended_runtime: String,
        install_script: String,
        install_instruction: String,
    },
}

/// Complete hardware and runtime audit result for a physical device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HardwareAudit {
    pub hardware: DetectedHardware,
    pub device_name: String,
    pub pci_id: Option<String>,
    pub status: RuntimeStatus,
}

impl HardwareAudit {
    #[must_use]
    pub const fn is_runtime_installed(&self) -> bool {
        matches!(self.status, RuntimeStatus::Installed { .. })
    }
}

/// Tracks recommendation prompts issued to the user for missing runtimes.
///
/// Ensures the user is advised up to `max_prompts` times (default: 3),
/// after which further prompts are suppressed and the system continues
/// with the next available backend in priority order.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeRecommendationTracker {
    prompt_counts: HashMap<DetectedHardware, u32>,
    max_prompts: u32,
}

impl Default for RuntimeRecommendationTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl RuntimeRecommendationTracker {
    #[must_use]
    pub fn new() -> Self {
        Self {
            prompt_counts: HashMap::new(),
            max_prompts: MAX_RUNTIME_INSTALL_PROMPTS,
        }
    }

    #[must_use]
    pub fn with_max_prompts(max_prompts: u32) -> Self {
        Self {
            prompt_counts: HashMap::new(),
            max_prompts,
        }
    }

    #[must_use]
    pub fn prompt_count(&self, hardware: DetectedHardware) -> u32 {
        self.prompt_counts.get(&hardware).copied().unwrap_or(0)
    }

    #[must_use]
    pub fn should_prompt(&self, hardware: DetectedHardware) -> bool {
        self.prompt_count(hardware) < self.max_prompts
    }

    /// Records a recommendation attempt for the given hardware.
    /// Returns `Some(recommendation_message)` if the prompt count is within the threshold ($\le 3$).
    /// Returns `None` once the threshold is exceeded, signaling silent continuation with the next backend.
    pub fn record_prompt(&mut self, hardware: DetectedHardware) -> Option<String> {
        let count = self.prompt_counts.entry(hardware).or_insert(0);
        *count += 1;
        let current = *count;

        if current <= self.max_prompts {
            let msg = format!(
                "Aviso [{current}/{}]: Hardware físico detectado ({}), mas a runtime '{}' não está instalada. \
                Para máximo desempenho, execute `{}`. Continuando com o próximo backend disponível.",
                self.max_prompts,
                hardware.name(),
                hardware.recommended_runtime(),
                hardware.install_script(),
            );
            Some(msg)
        } else {
            // Threshold exceeded: suppress further prompts and silently continue.
            None
        }
    }
}

/// Safe hardware detector that inspects PCI buses and driver interfaces.
pub struct HardwareScanner;

impl HardwareScanner {
    /// Detects physical hardware present on the system and audits runtime availability.
    #[must_use]
    #[allow(clippy::too_many_lines)]
    pub fn scan() -> Vec<HardwareAudit> {
        let mut audits = Vec::new();

        // 1. Check NVIDIA GPU
        if Self::has_nvidia_pci() || Path::new("/dev/nvidia0").exists() {
            let installed = Self::check_library(&[
                "/usr/lib64/libnvinfer.so",
                "/usr/lib64/libnvinfer.so.10",
                "/usr/lib64/libnvinfer.so.11",
                "/usr/lib/x86_64-linux-gnu/libnvinfer.so",
                "/usr/local/cuda/lib64/libnvinfer.so",
                "nvinfer.dll",
            ]);
            let status = if let Some(path) = installed {
                RuntimeStatus::Installed {
                    runtime_name: "TensorRT".to_owned(),
                    path: Some(path),
                }
            } else {
                RuntimeStatus::Missing {
                    recommended_runtime: "TensorRT".to_owned(),
                    install_script: "scripts/install-tensorrt.sh".to_owned(),
                    install_instruction: "Execute `./scripts/install-tensorrt.sh` para instalar a runtime TensorRT da NVIDIA.".to_owned(),
                }
            };
            audits.push(HardwareAudit {
                hardware: DetectedHardware::NvidiaGpu,
                device_name: "NVIDIA Dedicated GPU".to_owned(),
                pci_id: Some("10de".to_owned()),
                status,
            });
        }

        // 2. Check Intel NPU
        if Self::has_intel_npu() {
            let installed = Self::check_library(&[
                "/lib64/libopenvino.so.2510",
                "/usr/lib64/libopenvino.so",
                "/usr/lib/x86_64-linux-gnu/libopenvino.so",
                "openvino.dll",
            ]);
            let status = if let Some(path) = installed {
                RuntimeStatus::Installed {
                    runtime_name: "OpenVINO (NPU)".to_owned(),
                    path: Some(path),
                }
            } else {
                RuntimeStatus::Missing {
                    recommended_runtime: "OpenVINO".to_owned(),
                    install_script: "scripts/install-openvino.sh".to_owned(),
                    install_instruction: "Execute `./scripts/install-openvino.sh` para instalar o OpenVINO e o driver Intel NPU.".to_owned(),
                }
            };
            audits.push(HardwareAudit {
                hardware: DetectedHardware::IntelNpu,
                device_name: "Intel NPU".to_owned(),
                pci_id: Some("8086:7d1d".to_owned()),
                status,
            });
        }

        // 3. Check AMD NPU
        if Self::has_amd_npu() {
            let installed = Self::check_library(&[
                "/opt/xilinx/xrt/lib/libxrt_core.so",
                "/usr/lib64/libxrt_core.so",
                "xrt_core.dll",
            ]);
            let status = if let Some(path) = installed {
                RuntimeStatus::Installed {
                    runtime_name: "Ryzen AI (XDNA)".to_owned(),
                    path: Some(path),
                }
            } else {
                RuntimeStatus::Missing {
                    recommended_runtime: "Ryzen AI Software".to_owned(),
                    install_script: "scripts/install-ryzenai.sh".to_owned(),
                    install_instruction: "Execute `./scripts/install-ryzenai.sh` para instalar o driver XDNA e o runtime Ryzen AI.".to_owned(),
                }
            };
            audits.push(HardwareAudit {
                hardware: DetectedHardware::AmdNpu,
                device_name: "AMD Ryzen AI NPU".to_owned(),
                pci_id: Some("1022:1502".to_owned()),
                status,
            });
        }

        // 4. Check Intel GPU (Arc or iGPU)
        if Self::has_intel_gpu() {
            let installed = Self::check_library(&[
                "/lib64/libopenvino.so.2510",
                "/usr/lib64/libopenvino.so",
                "/usr/lib/x86_64-linux-gnu/libopenvino.so",
                "openvino.dll",
            ]);
            let status = if let Some(path) = installed {
                RuntimeStatus::Installed {
                    runtime_name: "OpenVINO (GPU)".to_owned(),
                    path: Some(path),
                }
            } else {
                RuntimeStatus::Missing {
                    recommended_runtime: "OpenVINO".to_owned(),
                    install_script: "scripts/install-openvino.sh".to_owned(),
                    install_instruction: "Execute `./scripts/install-openvino.sh` para habilitar aceleração de GPU Intel Arc / iGPU.".to_owned(),
                }
            };
            audits.push(HardwareAudit {
                hardware: DetectedHardware::IntelGpu,
                device_name: "Intel Arc / iGPU".to_owned(),
                pci_id: Some("8086:7dd1".to_owned()),
                status,
            });
        }

        audits
    }

    /// Evaluates candidate hardware audits against a recommendation tracker,
    /// returning active recommendations (up to 3 times per device) and the resolved backend.
    #[must_use]
    pub fn audit_and_recommend(
        tracker: &mut RuntimeRecommendationTracker,
        audits: &[HardwareAudit],
    ) -> Vec<String> {
        let mut messages = Vec::new();
        for audit in audits {
            if !audit.is_runtime_installed() {
                if let Some(msg) = tracker.record_prompt(audit.hardware) {
                    messages.push(msg);
                }
            }
        }
        messages
    }

    /// Evaluates candidate hardware audits against a recommendation tracker,
    /// returning the highest-tier available backend and any new user recommendations (up to 3 times per device).
    ///
    /// Priority order:
    /// 1. Dedicated GPU:
    ///    - TensorRT (if installed)
    ///    - If NVIDIA GPU detected without TensorRT: prompt user up to 3 times to install TensorRT.
    /// 2. NPU:
    ///    - Intel NPU via OpenVINO (if installed)
    ///    - AMD NPU via Ryzen AI / XDNA (if installed)
    ///    - If Intel/AMD NPU detected without runtime: prompt user up to 3 times to install runtime.
    /// 3. Integrated GPU:
    ///    - Intel Arc / iGPU via OpenVINO GPU (if installed)
    ///    - AMD Radeon iGPU via Ryzen AI (if installed)
    /// 4. CPU:
    ///    - Tract pure Rust fail-safe CPU baseline (always available)
    #[must_use]
    pub fn resolve_audits_to_backend(
        tracker: &mut RuntimeRecommendationTracker,
        audits: &[HardwareAudit],
    ) -> (crate::auto::BackendSelection, Vec<String>) {
        let mut messages = Vec::new();
        let mut selected = None;

        // Tier 1: Dedicated GPU (TensorRT first)
        for audit in audits {
            if audit.hardware == DetectedHardware::NvidiaGpu {
                if audit.is_runtime_installed() {
                    selected = Some(crate::auto::BackendSelection::TensorRt);
                    break;
                } else if let Some(msg) = tracker.record_prompt(audit.hardware) {
                    messages.push(msg);
                }
            }
        }

        // Tier 2: NPU (Intel NPU or AMD NPU)
        if selected.is_none() {
            for audit in audits {
                match audit.hardware {
                    DetectedHardware::IntelNpu => {
                        if audit.is_runtime_installed() {
                            selected = Some(crate::auto::BackendSelection::OpenVinoNpu);
                            break;
                        } else if let Some(msg) = tracker.record_prompt(audit.hardware) {
                            messages.push(msg);
                        }
                    }
                    DetectedHardware::AmdNpu => {
                        if audit.is_runtime_installed() {
                            selected = Some(crate::auto::BackendSelection::RyzenAiNpu);
                            break;
                        } else if let Some(msg) = tracker.record_prompt(audit.hardware) {
                            messages.push(msg);
                        }
                    }
                    _ => {}
                }
            }
        }

        // Tier 3: Integrated GPU (Intel Arc / iGPU or AMD iGPU)
        if selected.is_none() {
            for audit in audits {
                match audit.hardware {
                    DetectedHardware::IntelGpu => {
                        if audit.is_runtime_installed() {
                            selected = Some(crate::auto::BackendSelection::OpenVinoGpu);
                            break;
                        } else if let Some(msg) = tracker.record_prompt(audit.hardware) {
                            messages.push(msg);
                        }
                    }
                    DetectedHardware::AmdGpu => {
                        if audit.is_runtime_installed() {
                            selected = Some(crate::auto::BackendSelection::RyzenAiGpu);
                            break;
                        }
                    }
                    _ => {}
                }
            }
        }

        // Tier 4: CPU (Tract pure Rust CPU baseline)
        let final_selection = selected.unwrap_or(crate::auto::BackendSelection::TractCpu);
        (final_selection, messages)
    }

    /// Convenience scanner that scans host hardware, prompts missing runtimes up to 3 times,
    /// and resolves the highest available backend in priority order.
    #[must_use]
    pub fn select_backend_with_recommendations(
        tracker: &mut RuntimeRecommendationTracker,
    ) -> (crate::auto::BackendSelection, Vec<String>) {
        let audits = Self::scan();
        Self::resolve_audits_to_backend(tracker, &audits)
    }

    fn has_nvidia_pci() -> bool {
        Self::check_pci_vendor("0x10de")
    }

    fn has_intel_npu() -> bool {
        Path::new("/dev/accel/accel0").exists()
            || Path::new("/sys/class/accel").exists()
            || Self::check_pci_vendor_and_device("0x8086", "0x7d1d")
            || Self::check_pci_vendor_and_device("0x8086", "0x7d1e")
    }

    fn has_amd_npu() -> bool {
        Path::new("/dev/amdxdna").exists() || Self::check_pci_vendor_and_device("0x1022", "0x1502")
    }

    fn has_intel_gpu() -> bool {
        Self::check_pci_vendor("0x8086")
    }

    fn check_pci_vendor(vendor_hex: &str) -> bool {
        let pci_dir = Path::new("/sys/bus/pci/devices");
        if let Ok(entries) = fs::read_dir(pci_dir) {
            for entry in entries.flatten() {
                let vendor_path = entry.path().join("vendor");
                if let Ok(content) = fs::read_to_string(vendor_path) {
                    if content.trim().eq_ignore_ascii_case(vendor_hex) {
                        return true;
                    }
                }
            }
        }
        false
    }

    fn check_pci_vendor_and_device(vendor_hex: &str, device_hex: &str) -> bool {
        let pci_dir = Path::new("/sys/bus/pci/devices");
        if let Ok(entries) = fs::read_dir(pci_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let v_ok = fs::read_to_string(path.join("vendor"))
                    .map(|v| v.trim().eq_ignore_ascii_case(vendor_hex))
                    .unwrap_or(false);
                let d_ok = fs::read_to_string(path.join("device"))
                    .map(|d| d.trim().eq_ignore_ascii_case(device_hex))
                    .unwrap_or(false);
                if v_ok && d_ok {
                    return true;
                }
            }
        }
        false
    }

    fn check_library(candidates: &[&str]) -> Option<String> {
        for candidate in candidates {
            if Path::new(candidate).exists() {
                return Some((*candidate).to_owned());
            }
        }
        None
    }
}
