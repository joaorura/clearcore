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

    /// Whether the runtime recommended for this hardware runs real inference in this build.
    ///
    /// Only the `OpenVINO` hardware (Intel NPU / GPU) and the CPU baseline do. `TensorRT`, Ryzen AI,
    /// `DirectML` / Vulkan and `CoreML` are detected but only copy samples today (see
    /// [`crate::BackendSelection::executes_inference`]), so installing their runtime would not make
    /// anything faster and the user must not be told to.
    #[must_use]
    pub const fn runs_inference(self) -> bool {
        matches!(self, Self::IntelNpu | Self::IntelGpu | Self::GenericCpu)
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
            let msg = if hardware.runs_inference() {
                format!(
                    "Aviso [{current}/{}]: Hardware físico detectado ({}), mas a runtime '{}' não está instalada. \
                    Para máximo desempenho, execute `{}`. Continuando com o próximo backend disponível.",
                    self.max_prompts,
                    hardware.name(),
                    hardware.recommended_runtime(),
                    hardware.install_script(),
                )
            } else {
                // Installing the runtime would not help: there is no inference path for it yet.
                format!(
                    "Aviso [{current}/{}]: Hardware físico detectado ({}), mas o caminho de inferência \
                    para '{}' ainda não está implementado; instalar a runtime não muda o desempenho. \
                    Continuando com o próximo backend disponível.",
                    self.max_prompts,
                    hardware.name(),
                    hardware.recommended_runtime(),
                )
            };
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
            // Same discovery the runtime uses to load libnvinfer (`CLEARCORE_TENSORRT_PATH` /
            // `TENSORRT_LIB_PATH`, then system locations), so detection never disagrees with
            // what `TensorRtBackend` can actually open.
            let installed =
                realtime_noise_runtime_tensorrt::locate_tensorrt_library().or_else(|| {
                    Self::check_library(&[
                        "/opt/tensorrt/lib/libnvinfer.so.11",
                        "/opt/tensorrt/lib/libnvinfer.so",
                        "/usr/local/tensorrt/lib/libnvinfer.so.11",
                        "/usr/local/tensorrt/lib/libnvinfer.so",
                        "/usr/lib64/libnvinfer.so",
                        "/usr/lib64/libnvinfer.so.10",
                        "/usr/lib64/libnvinfer.so.11",
                        "/usr/lib/x86_64-linux-gnu/libnvinfer.so",
                        "/usr/lib/x86_64-linux-gnu/libnvinfer.so.11",
                        "/usr/local/cuda/lib64/libnvinfer.so",
                        "nvinfer.dll",
                        "nvinfer_11.dll",
                    ])
                });
            let status = if let Some(path) = installed {
                RuntimeStatus::Installed {
                    runtime_name: "TensorRT".to_owned(),
                    path: Some(path),
                }
            } else {
                Self::missing_status(
                    DetectedHardware::NvidiaGpu,
                    "TensorRT",
                    "Execute `./scripts/install-tensorrt.sh` para instalar a runtime TensorRT da NVIDIA.",
                )
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
            let installed = Self::find_openvino_library();
            let status = if let Some(path) = installed {
                RuntimeStatus::Installed {
                    runtime_name: "OpenVINO (NPU)".to_owned(),
                    path: Some(path),
                }
            } else {
                Self::missing_status(
                    DetectedHardware::IntelNpu,
                    "OpenVINO",
                    "Execute `./scripts/install-openvino.sh` para instalar o OpenVINO e o driver Intel NPU.",
                )
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
                Self::missing_status(
                    DetectedHardware::AmdNpu,
                    "Ryzen AI Software",
                    "Execute `./scripts/install-ryzenai.sh` para instalar o driver XDNA e o runtime Ryzen AI.",
                )
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
            let installed = Self::find_openvino_library();
            let status = if let Some(path) = installed {
                RuntimeStatus::Installed {
                    runtime_name: "OpenVINO (GPU)".to_owned(),
                    path: Some(path),
                }
            } else {
                Self::missing_status(
                    DetectedHardware::IntelGpu,
                    "OpenVINO",
                    "Execute `./scripts/install-openvino.sh` para habilitar aceleração de GPU Intel Arc / iGPU.",
                )
            };
            audits.push(HardwareAudit {
                hardware: DetectedHardware::IntelGpu,
                device_name: "Intel Arc / iGPU".to_owned(),
                pci_id: Some("8086:7dd1".to_owned()),
                status,
            });
        }

        // 5. Check AMD GPU (Radeon dGPU or iGPU)
        if Self::has_amd_gpu() {
            let installed = Self::check_library(&[
                "/usr/lib64/libvulkan.so.1",
                "/usr/lib/x86_64-linux-gnu/libvulkan.so.1",
                "DirectML.dll",
                "vulkan-1.dll",
            ]);
            let status = Self::amd_gpu_status(installed);
            audits.push(HardwareAudit {
                hardware: DetectedHardware::AmdGpu,
                device_name: "AMD Radeon GPU / iGPU".to_owned(),
                pci_id: Some("1002".to_owned()),
                status,
            });
        }

        // 6. Check Apple Silicon CoreML (macOS ANE / Metal GPU)
        if cfg!(target_os = "macos") {
            let is_arm = cfg!(target_arch = "aarch64");
            audits.push(HardwareAudit {
                hardware: if is_arm {
                    DetectedHardware::AppleSilicon
                } else {
                    DetectedHardware::GenericCpu
                },
                device_name: if is_arm {
                    "Apple Silicon (ANE / GPU)".to_owned()
                } else {
                    "Apple Intel CPU".to_owned()
                },
                pci_id: None,
                status: RuntimeStatus::Installed {
                    runtime_name: "CoreML".to_owned(),
                    path: Some("/System/Library/Frameworks/CoreML.framework".to_owned()),
                },
            });
        }

        audits
    }

    /// `Missing` status for `hardware` whose vendor runtime was not found.
    ///
    /// Only hardware that [`DetectedHardware::runs_inference`] gets an install script and
    /// instruction. For the rest (`TensorRT`, Ryzen AI, ...) installing the runtime would not make
    /// anything faster, so the status carries no script and an honest "detected; inference path not
    /// implemented yet" instruction instead of telling the user to install something useless.
    fn missing_status(
        hardware: DetectedHardware,
        recommended_runtime: &str,
        install_instruction: &str,
    ) -> RuntimeStatus {
        if hardware.runs_inference() {
            RuntimeStatus::Missing {
                recommended_runtime: recommended_runtime.to_owned(),
                install_script: hardware.install_script().to_owned(),
                install_instruction: install_instruction.to_owned(),
            }
        } else {
            RuntimeStatus::Missing {
                recommended_runtime: recommended_runtime.to_owned(),
                install_script: String::new(),
                install_instruction: format!(
                    "{} detectado; o caminho de inferência para {recommended_runtime} ainda não \
                     está implementado, então não há nada para instalar.",
                    hardware.name(),
                ),
            }
        }
    }

    /// Runtime status of an AMD Radeon GPU given the GPU-compute library found on the host.
    ///
    /// Finding `libvulkan` / `DirectML.dll` only proves the loader exists. Neither the Vulkan nor
    /// the DirectML backend opens a device or runs a kernel (their `executes_inference()` is
    /// `false`), so the label says "detected", not "DirectML / Vulkan Installed", and
    /// [`Self::resolve_audits_to_backend`] never selects a backend for it.
    fn amd_gpu_status(library: Option<String>) -> RuntimeStatus {
        match library {
            Some(path) => RuntimeStatus::Installed {
                runtime_name: "Vulkan / DirectML loader (detected only; no inference path wired)"
                    .to_owned(),
                path: Some(path),
            },
            None => RuntimeStatus::Missing {
                recommended_runtime: "DirectML / Vulkan".to_owned(),
                install_script: "scripts/detect-hardware.sh".to_owned(),
                install_instruction:
                    "Instale os drivers Vulkan / DirectML para a GPU AMD Radeon. Hoje isso só \
                     habilita a detecção: a GPU AMD ainda não executa inferência."
                        .to_owned(),
            },
        }
    }

    /// `selection` if the runtime behind it runs real inference in this build, else `None`.
    ///
    /// Hardware that is present and has its runtime installed is still skipped when the backend
    /// only copies samples (see [`crate::BackendSelection::executes_inference`]); the resolver
    /// then moves on to the next tier instead of handing back a backend that does not denoise.
    fn usable(selection: crate::auto::BackendSelection) -> Option<crate::auto::BackendSelection> {
        selection.executes_inference().then_some(selection)
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
    /// Priority order, considering only backends that run real inference
    /// ([`crate::BackendSelection::executes_inference`]):
    /// 1. Dedicated GPU:
    ///    - TensorRT, once it executes an engine (today it only does a CUDA copy, so it is skipped)
    ///    - If NVIDIA GPU detected without TensorRT: prompt user up to 3 times to install TensorRT.
    /// 2. NPU:
    ///    - Intel NPU via OpenVINO (if installed)
    ///    - AMD NPU via Ryzen AI / XDNA and Apple Neural Engine via CoreML, once they execute
    ///      (today they only copy samples, so they are skipped)
    ///    - If Intel/AMD NPU detected without runtime: prompt user up to 3 times to install runtime.
    /// 3. Integrated GPU:
    ///    - Intel Arc / iGPU via OpenVINO GPU (if installed)
    ///    - AMD Radeon GPUs have no inference path yet and select nothing.
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
                    if let Some(selection) = Self::usable(crate::auto::BackendSelection::TensorRt) {
                        selected = Some(selection);
                        break;
                    }
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
                            if let Some(selection) =
                                Self::usable(crate::auto::BackendSelection::RyzenAiNpu)
                            {
                                selected = Some(selection);
                                break;
                            }
                        } else if let Some(msg) = tracker.record_prompt(audit.hardware) {
                            messages.push(msg);
                        }
                    }
                    DetectedHardware::AppleSilicon => {
                        if audit.is_runtime_installed()
                            && let Some(selection) =
                                Self::usable(crate::auto::BackendSelection::CoreMl)
                        {
                            selected = Some(selection);
                            break;
                        }
                    }
                    _ => {}
                }
            }
        }

        // Tier 3: Integrated GPU (Intel Arc / iGPU)
        if selected.is_none() {
            for audit in audits {
                // Only Intel Arc / iGPU has an inference path. AMD Radeon GPUs are skipped: only
                // the Vulkan/DirectML loader is detected and neither backend runs inference, and
                // `RyzenAiGpu` is the Ryzen AI iGPU path, not a generic Radeon one.
                if audit.hardware == DetectedHardware::IntelGpu {
                    if audit.is_runtime_installed() {
                        selected = Some(crate::auto::BackendSelection::OpenVinoGpu);
                        break;
                    } else if let Some(msg) = tracker.record_prompt(audit.hardware) {
                        messages.push(msg);
                    }
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

    fn has_amd_gpu() -> bool {
        Self::check_pci_vendor("0x1002")
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

    /// Locates the OpenVINO runtime the same way `OpenVINOBackend` loads it (the C API library
    /// `libopenvino_c`), falling back to the core library paths used by older installers.
    fn find_openvino_library() -> Option<String> {
        realtime_noise_runtime_openvino::locate_library()
            .map(|path| path.display().to_string())
            .or_else(|| {
                Self::check_library(&[
                    "/lib64/libopenvino.so.2510",
                    "/usr/lib64/libopenvino.so",
                    "/usr/lib/x86_64-linux-gnu/libopenvino.so",
                    "openvino.dll",
                ])
            })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn amd_gpu_with_a_loader_is_reported_as_detected_not_as_a_working_runtime() {
        let status = HardwareScanner::amd_gpu_status(Some("/usr/lib64/libvulkan.so.1".to_owned()));
        let RuntimeStatus::Installed { runtime_name, path } = status else {
            unreachable!("a found loader is reported as installed");
        };
        assert_eq!(path.as_deref(), Some("/usr/lib64/libvulkan.so.1"));
        assert!(runtime_name.contains("detected only"), "{runtime_name}");
        assert!(runtime_name.contains("no inference path"), "{runtime_name}");
        assert_ne!(runtime_name, "DirectML / Vulkan");
    }

    /// `(install_script, install_instruction)` of a `Missing` status.
    fn missing_parts(status: RuntimeStatus) -> Option<(String, String)> {
        match status {
            RuntimeStatus::Missing {
                install_script,
                install_instruction,
                ..
            } => Some((install_script, install_instruction)),
            RuntimeStatus::Installed { .. } => None,
        }
    }

    #[test]
    fn missing_runtime_without_an_inference_path_does_not_ask_for_an_install() {
        for hardware in [DetectedHardware::NvidiaGpu, DetectedHardware::AmdNpu] {
            let status =
                HardwareScanner::missing_status(hardware, "Qualquer", "Execute `./scripts/x.sh`");
            let (script, instruction) = missing_parts(status).unwrap_or_default();
            assert!(script.is_empty(), "{hardware:?}: {script}");
            assert!(instruction.contains("detectado"), "{instruction}");
            assert!(
                instruction.contains("ainda não está implementado"),
                "{instruction}"
            );
            assert!(!instruction.contains("scripts/"), "{instruction}");
        }
    }

    #[test]
    fn missing_runtime_with_an_inference_path_keeps_the_install_script() {
        let status = HardwareScanner::missing_status(
            DetectedHardware::IntelNpu,
            "OpenVINO",
            "Execute `./scripts/install-openvino.sh`",
        );
        let (script, instruction) = missing_parts(status).unwrap_or_default();
        assert_eq!(script, "scripts/install-openvino.sh");
        assert!(instruction.contains("install-openvino.sh"));
    }

    #[test]
    fn amd_gpu_without_a_loader_is_missing() {
        assert!(matches!(
            HardwareScanner::amd_gpu_status(None),
            RuntimeStatus::Missing { .. }
        ));
    }
}
