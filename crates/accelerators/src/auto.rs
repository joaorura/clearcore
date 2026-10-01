#![forbid(unsafe_code)]
#![allow(clippy::doc_markdown)]

use serde::{Deserialize, Serialize};

/// Maximum allowable p99 latency for qualification promotion (10.0 ms @ 48 kHz).
pub const QUALIFICATION_MAX_P99_MS: f64 = 10.0;

/// Hard deadline for inference per hop (10.0 ms @ 48 kHz).
pub const QUALIFICATION_MAX_DEADLINE_MS: f64 = 10.0;

/// Decision on whether an accelerator backend is promoted for AUTO selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PromotionDecision {
    Promoted,
    NotPromoted,
}

impl PromotionDecision {
    #[must_use]
    pub const fn is_promoted(self) -> bool {
        matches!(self, Self::Promoted)
    }
}

/// Hardware execution device tier for AUTO prioritization.
/// Priority order:
/// 1. Dedicated GPU (`TensorRT` specific runtime, fallback to `DirectML`/`Vulkan` general runtime)
/// 2. NPU (Intel NPU via `OpenVINO`, AMD NPU via Ryzen AI / XDNA, Apple Neural Engine via `CoreML`)
/// 3. Integrated GPU (Intel Arc / iGPU via `OpenVINO` GPU, AMD iGPU via Ryzen AI / `Vulkan`/`DirectML`)
/// 4. CPU (Intel CPU prefers `OpenVINO` AMX/VNNI, generic CPU uses Tract pure Rust fail-safe baseline)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum DeviceTier {
    DedicatedGpu = 1,
    Npu = 2,
    IntegratedGpu = 3,
    Cpu = 4,
}

/// Requested backend mode.
///
/// Allows the user to select by category (`DedicatedGpu`, `Npu`, `IntegratedGpu`, `Cpu`)
/// or by specific runtime (`TensorRt`, `DirectMl`, `Vulkan`, `RyzenAiNpu`, `RyzenAiGpu`, `RyzenAi`, `OpenVinoNpu`, `OpenVinoGpu`, `OpenVinoCpu`, `CoreMl`, `TractCpu`),
/// or leave on Auto.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum BackendRequest {
    #[default]
    Auto,
    // Device Category Requests
    DedicatedGpu,
    Npu,
    IntegratedGpu,
    Cpu,
    // Specific Runtime Requests
    TensorRt,
    Cuda,
    DirectMl,
    Vulkan,
    RyzenAiNpu,
    RyzenAiGpu,
    RyzenAi,
    OpenVinoNpu,
    OpenVinoGpu,
    OpenVinoCpu,
    OpenVino,
    CoreMl,
    TractCpu,
}

/// Backend selected by the AUTO policy or direct user request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum BackendSelection {
    #[default]
    TractCpu,
    TensorRt,
    DirectMl,
    Vulkan,
    RyzenAiNpu,
    RyzenAiGpu,
    RyzenAi,
    OpenVinoNpu,
    OpenVinoGpu,
    OpenVinoCpu,
    OpenVino,
    CoreMl,
}

impl BackendSelection {
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::TractCpu => "tract",
            Self::TensorRt => "tensorrt",
            Self::DirectMl => "directml",
            Self::Vulkan => "vulkan",
            Self::RyzenAiNpu => "ryzenai-npu",
            Self::RyzenAiGpu => "ryzenai-gpu",
            Self::RyzenAi => "ryzen-ai",
            Self::OpenVinoNpu => "openvino-npu",
            Self::OpenVinoGpu => "openvino-gpu",
            Self::OpenVinoCpu => "openvino-cpu",
            Self::OpenVino => "openvino",
            Self::CoreMl => "coreml",
        }
    }

    #[must_use]
    pub const fn tier(&self) -> DeviceTier {
        match self {
            Self::TensorRt | Self::DirectMl | Self::Vulkan => DeviceTier::DedicatedGpu,
            Self::RyzenAiNpu | Self::RyzenAi | Self::OpenVinoNpu | Self::CoreMl => DeviceTier::Npu,
            Self::RyzenAiGpu | Self::OpenVinoGpu => DeviceTier::IntegratedGpu,
            Self::OpenVinoCpu | Self::TractCpu | Self::OpenVino => DeviceTier::Cpu,
        }
    }

    #[must_use]
    pub const fn is_tract_cpu(&self) -> bool {
        matches!(self, Self::TractCpu)
    }

    #[must_use]
    pub const fn is_nvidia(&self) -> bool {
        matches!(self, Self::TensorRt)
    }

    #[must_use]
    pub const fn is_dedicated_gpu(&self) -> bool {
        matches!(self, Self::TensorRt | Self::DirectMl | Self::Vulkan)
    }

    #[must_use]
    pub const fn is_npu(&self) -> bool {
        matches!(
            self,
            Self::RyzenAiNpu | Self::RyzenAi | Self::OpenVinoNpu | Self::CoreMl
        )
    }

    #[must_use]
    pub const fn is_amd(&self) -> bool {
        matches!(self, Self::RyzenAiNpu | Self::RyzenAiGpu | Self::RyzenAi)
    }

    #[must_use]
    pub const fn is_intel(&self) -> bool {
        matches!(
            self,
            Self::OpenVinoNpu | Self::OpenVinoGpu | Self::OpenVinoCpu | Self::OpenVino
        )
    }

    #[must_use]
    pub const fn is_integrated_gpu(&self) -> bool {
        matches!(self, Self::OpenVinoGpu | Self::RyzenAiGpu)
    }

    #[must_use]
    pub const fn is_gpu(&self) -> bool {
        matches!(
            self,
            Self::TensorRt | Self::DirectMl | Self::Vulkan | Self::OpenVinoGpu | Self::RyzenAiGpu
        )
    }
}

/// Offline calibration report generated outside the real-time audio pipeline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CalibrationReport {
    pub backend_name: String,
    pub duration_seconds: f64,
    pub total_frames: usize,
    pub p50_latency_ms: f64,
    pub p95_latency_ms: f64,
    pub p99_latency_ms: f64,
    pub max_latency_ms: f64,
    pub deadline_miss_count: usize,
    pub discontinuities: usize,
    pub decision: PromotionDecision,
    pub reason: Option<String>,
}

impl CalibrationReport {
    /// Validates whether the calibration report meets all qualification thresholds.
    #[must_use]
    pub fn is_promoted(&self) -> bool {
        self.decision == PromotionDecision::Promoted
            && self.p99_latency_ms <= QUALIFICATION_MAX_P99_MS
            && self.max_latency_ms <= QUALIFICATION_MAX_DEADLINE_MS
            && self.deadline_miss_count == 0
            && self.discontinuities == 0
    }
}

/// Priority score for candidate backends in AUTO selection.
/// Higher score indicates higher selection priority:
/// - Dedicated GPU:
///   * Proprietary / Vendor-Specific GPU runtimes: `TensorRT` score 100, `CUDA` score 95
///   * General / Universal GPU runtimes: `DirectML` on dGPU score 90, `Vulkan` score 85
/// - NPU: Intel NPU via `OpenVINO` score 80; AMD NPU via Ryzen AI / XDNA score 80; Apple Neural Engine via `CoreML` 75
/// - Integrated GPU: Intel Arc / iGPU via `OpenVINO` GPU score 70; AMD iGPU via Ryzen AI score 70; DirectML on iGPU score 70
/// - CPU: Intel CPU prefers `OpenVINO` CPU AMX/VNNI score 60; Tract pure Rust CPU baseline score 50
#[must_use]
pub fn backend_priority_score(backend_name: &str) -> u32 {
    match backend_name.to_ascii_lowercase().as_str() {
        // Tier 1: Dedicated GPU - Proprietary / Vendor-Specific runtimes (NVIDIA TensorRT)
        "tensorrt" | "cuda" | "nvidia" => 100,
        // Tier 1: Dedicated GPU - General / Universal runtimes (DirectML below proprietary runtimes, Vulkan)
        "directml" | "directml-dgpu" | "dx12" => 90,
        "vulkan" | "vulkan-dgpu" => 85,
        // Tier 2: NPU (Intel NPU via OpenVINO, AMD NPU via Ryzen AI / XDNA)
        "openvino-npu" | "intel-npu" | "npu" | "ryzenai-npu" | "ryzen-ai" | "ryzenai"
        | "amd-npu" | "vitisai" | "xdna" => 80,
        // Tier 2: Apple Neural Engine via CoreML
        "coreml" | "ane" => 75,
        // Tier 3: Integrated GPU (Intel Arc / iGPU via OpenVINO GPU, AMD iGPU via Ryzen AI, DirectML iGPU)
        "openvino-gpu" | "intel-gpu" | "arc" | "ryzenai-gpu" | "ryzen-ai-gpu" | "amd-igpu"
        | "rdna-igpu" | "directml-igpu" => 70,
        "openvino" => 65,
        // Tier 4: CPU - Intel CPU prefers OpenVINO (score 60) over ONNX / Tract baseline (score 50)
        "openvino-cpu" | "intel-cpu" => 60,
        "tract" | "cpu" | "onnx" => 50,
        _ => 10,
    }
}

/// Maps a backend name to its canonical `BackendSelection`.
#[must_use]
pub fn backend_selection_from_name(backend_name: &str) -> BackendSelection {
    match backend_name.to_ascii_lowercase().as_str() {
        "tensorrt" | "cuda" | "nvidia" => BackendSelection::TensorRt,
        "vulkan" => BackendSelection::Vulkan,
        "directml" | "dx12" => BackendSelection::DirectMl,
        "ryzenai-npu" | "ryzen-ai" | "ryzenai" | "amd-npu" | "vitisai" | "xdna" => {
            BackendSelection::RyzenAiNpu
        }
        "ryzenai-gpu" | "ryzen-ai-gpu" | "amd-igpu" | "rdna-igpu" => BackendSelection::RyzenAiGpu,
        "openvino-npu" | "intel-npu" | "npu" => BackendSelection::OpenVinoNpu,
        "openvino-gpu" | "intel-gpu" | "arc" => BackendSelection::OpenVinoGpu,
        "openvino-cpu" | "intel-cpu" => BackendSelection::OpenVinoCpu,
        "openvino" => BackendSelection::OpenVino,
        "coreml" | "ane" => BackendSelection::CoreMl,
        _ => BackendSelection::TractCpu,
    }
}

/// Evaluates calibration metrics against frozen qualification thresholds.
#[allow(clippy::too_many_arguments)]
#[must_use]
pub fn evaluate_calibration(
    backend_name: &str,
    duration_seconds: f64,
    total_frames: usize,
    p50_latency_ms: f64,
    p95_latency_ms: f64,
    p99_latency_ms: f64,
    max_latency_ms: f64,
    deadline_miss_count: usize,
    discontinuities: usize,
) -> CalibrationReport {
    let mut reasons = Vec::new();

    if p99_latency_ms > QUALIFICATION_MAX_P99_MS {
        reasons.push(format!(
            "p99 latency {p99_latency_ms:.2} ms exceeds threshold {QUALIFICATION_MAX_P99_MS:.2} ms"
        ));
    }
    if max_latency_ms > QUALIFICATION_MAX_DEADLINE_MS {
        reasons.push(format!(
            "max latency {max_latency_ms:.2} ms exceeds deadline {QUALIFICATION_MAX_DEADLINE_MS:.2} ms"
        ));
    }
    if deadline_miss_count > 0 {
        reasons.push(format!(
            "{deadline_miss_count} deadline misses recorded (must be 0)"
        ));
    }
    if discontinuities > 0 {
        reasons.push(format!(
            "{discontinuities} discontinuities detected (must be 0)"
        ));
    }

    let (decision, reason) = if reasons.is_empty() {
        (PromotionDecision::Promoted, None)
    } else {
        (PromotionDecision::NotPromoted, Some(reasons.join("; ")))
    };

    CalibrationReport {
        backend_name: backend_name.to_owned(),
        duration_seconds,
        total_frames,
        p50_latency_ms,
        p95_latency_ms,
        p99_latency_ms,
        max_latency_ms,
        deadline_miss_count,
        discontinuities,
        decision,
        reason,
    }
}

/// Conservative AUTO selection policy:
/// If the calibration report fails any quality gate or is not promoted,
/// falls back safely to the warmed tract CPU baseline.
#[must_use]
#[allow(clippy::needless_pass_by_value)]
pub fn select_auto(report: CalibrationReport) -> BackendSelection {
    if !report.is_promoted() {
        return BackendSelection::TractCpu;
    }
    backend_selection_from_name(&report.backend_name)
}

/// Evaluates a list of candidate calibration reports and selects the best promoted backend
/// according to the 4-tier hierarchy:
/// 1. Dedicated GPU (`TensorRT` specific runtime > `DirectML` / `Vulkan` general runtime)
/// 2. NPU (Intel NPU via `OpenVINO` / AMD NPU via Ryzen AI / Apple Neural Engine)
/// 3. Integrated GPU (Intel Arc / iGPU via `OpenVINO` GPU, AMD iGPU via Ryzen AI)
/// 4. CPU (Intel CPU prefers `OpenVINO` CPU; generic CPU uses Tract CPU baseline)
#[must_use]
pub fn select_best(reports: &[CalibrationReport]) -> BackendSelection {
    reports
        .iter()
        .filter(|r| r.is_promoted())
        .max_by_key(|r| backend_priority_score(&r.backend_name))
        .map_or(BackendSelection::TractCpu, |r| {
            backend_selection_from_name(&r.backend_name)
        })
}

/// AUTO backend policy manager.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AutoPolicy {
    warmed_tract_available: bool,
}

impl AutoPolicy {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            warmed_tract_available: true,
        }
    }

    #[must_use]
    pub const fn is_warmed_tract_available(&self) -> bool {
        self.warmed_tract_available
    }

    #[must_use]
    pub fn select(&self, report: &CalibrationReport) -> BackendSelection {
        select_auto(report.clone())
    }

    #[must_use]
    pub fn select_best(&self, candidates: &[CalibrationReport]) -> BackendSelection {
        select_best(candidates)
    }

    #[must_use]
    #[allow(clippy::too_many_lines)]
    pub fn resolve_candidates(
        &self,
        request: BackendRequest,
        candidates: &[CalibrationReport],
    ) -> BackendSelection {
        match request {
            BackendRequest::Auto => select_best(candidates),
            BackendRequest::DedicatedGpu => candidates
                .iter()
                .filter(|r| {
                    if !r.is_promoted() {
                        return false;
                    }
                    backend_selection_from_name(&r.backend_name).is_dedicated_gpu()
                })
                .max_by_key(|r| backend_priority_score(&r.backend_name))
                .map_or(BackendSelection::TractCpu, |r| {
                    backend_selection_from_name(&r.backend_name)
                }),
            BackendRequest::Npu => candidates
                .iter()
                .filter(|r| {
                    if !r.is_promoted() {
                        return false;
                    }
                    let sel = backend_selection_from_name(&r.backend_name);
                    sel.is_npu() || r.backend_name.eq_ignore_ascii_case("openvino")
                })
                .max_by_key(|r| backend_priority_score(&r.backend_name))
                .map_or(BackendSelection::TractCpu, |r| {
                    let sel = backend_selection_from_name(&r.backend_name);
                    if matches!(sel, BackendSelection::OpenVino) {
                        BackendSelection::OpenVinoNpu
                    } else {
                        sel
                    }
                }),
            BackendRequest::IntegratedGpu => candidates
                .iter()
                .filter(|r| {
                    if !r.is_promoted() {
                        return false;
                    }
                    backend_selection_from_name(&r.backend_name).is_integrated_gpu()
                })
                .max_by_key(|r| backend_priority_score(&r.backend_name))
                .map_or(BackendSelection::TractCpu, |r| {
                    backend_selection_from_name(&r.backend_name)
                }),
            // Tier 4 CPU: Prefer OpenVINO CPU (AMX/VNNI) on Intel CPU; otherwise Tract pure Rust CPU
            BackendRequest::Cpu => candidates
                .iter()
                .filter(|r| {
                    if !r.is_promoted() {
                        return false;
                    }
                    matches!(
                        backend_selection_from_name(&r.backend_name),
                        BackendSelection::OpenVinoCpu | BackendSelection::TractCpu
                    )
                })
                .max_by_key(|r| backend_priority_score(&r.backend_name))
                .map_or(BackendSelection::TractCpu, |r| {
                    backend_selection_from_name(&r.backend_name)
                }),
            BackendRequest::TractCpu => BackendSelection::TractCpu,
            BackendRequest::TensorRt | BackendRequest::Cuda => candidates
                .iter()
                .find(|r| {
                    r.is_promoted()
                        && (r.backend_name.eq_ignore_ascii_case("tensorrt")
                            || r.backend_name.eq_ignore_ascii_case("cuda"))
                })
                .map_or(BackendSelection::TractCpu, |_| BackendSelection::TensorRt),
            BackendRequest::DirectMl => candidates
                .iter()
                .find(|r| r.is_promoted() && r.backend_name.eq_ignore_ascii_case("directml"))
                .map_or(BackendSelection::TractCpu, |_| BackendSelection::DirectMl),
            BackendRequest::Vulkan => candidates
                .iter()
                .find(|r| r.is_promoted() && r.backend_name.eq_ignore_ascii_case("vulkan"))
                .map_or(BackendSelection::TractCpu, |_| BackendSelection::Vulkan),
            BackendRequest::RyzenAiNpu => candidates
                .iter()
                .find(|r| {
                    r.is_promoted()
                        && (r.backend_name.eq_ignore_ascii_case("ryzenai-npu")
                            || r.backend_name.eq_ignore_ascii_case("ryzen-ai")
                            || r.backend_name.eq_ignore_ascii_case("ryzenai")
                            || r.backend_name.eq_ignore_ascii_case("amd-npu")
                            || r.backend_name.eq_ignore_ascii_case("vitisai")
                            || r.backend_name.eq_ignore_ascii_case("xdna"))
                })
                .map_or(BackendSelection::TractCpu, |_| BackendSelection::RyzenAiNpu),
            BackendRequest::RyzenAiGpu => candidates
                .iter()
                .find(|r| {
                    r.is_promoted()
                        && (r.backend_name.eq_ignore_ascii_case("ryzenai-gpu")
                            || r.backend_name.eq_ignore_ascii_case("ryzen-ai-gpu")
                            || r.backend_name.eq_ignore_ascii_case("amd-igpu")
                            || r.backend_name.eq_ignore_ascii_case("rdna-igpu"))
                })
                .map_or(BackendSelection::TractCpu, |_| BackendSelection::RyzenAiGpu),
            BackendRequest::RyzenAi => candidates
                .iter()
                .filter(|r| {
                    if !r.is_promoted() {
                        return false;
                    }
                    backend_selection_from_name(&r.backend_name).is_amd()
                })
                .max_by_key(|r| backend_priority_score(&r.backend_name))
                .map_or(BackendSelection::TractCpu, |r| {
                    backend_selection_from_name(&r.backend_name)
                }),
            BackendRequest::OpenVinoNpu => candidates
                .iter()
                .find(|r| {
                    r.is_promoted()
                        && (r.backend_name.eq_ignore_ascii_case("openvino-npu")
                            || r.backend_name.eq_ignore_ascii_case("npu")
                            || r.backend_name.eq_ignore_ascii_case("intel-npu")
                            || r.backend_name.eq_ignore_ascii_case("openvino"))
                })
                .map_or(BackendSelection::TractCpu, |_| {
                    BackendSelection::OpenVinoNpu
                }),
            BackendRequest::OpenVinoGpu => candidates
                .iter()
                .find(|r| {
                    r.is_promoted()
                        && (r.backend_name.eq_ignore_ascii_case("openvino-gpu")
                            || r.backend_name.eq_ignore_ascii_case("intel-gpu")
                            || r.backend_name.eq_ignore_ascii_case("arc"))
                })
                .map_or(BackendSelection::TractCpu, |_| {
                    BackendSelection::OpenVinoGpu
                }),
            BackendRequest::OpenVinoCpu => candidates
                .iter()
                .find(|r| {
                    r.is_promoted()
                        && (r.backend_name.eq_ignore_ascii_case("openvino-cpu")
                            || r.backend_name.eq_ignore_ascii_case("intel-cpu"))
                })
                .map_or(BackendSelection::TractCpu, |_| {
                    BackendSelection::OpenVinoCpu
                }),
            BackendRequest::OpenVino => candidates
                .iter()
                .find(|r| {
                    r.is_promoted()
                        && (r.backend_name.eq_ignore_ascii_case("openvino")
                            || r.backend_name.eq_ignore_ascii_case("openvino-npu")
                            || r.backend_name.eq_ignore_ascii_case("openvino-gpu")
                            || r.backend_name.eq_ignore_ascii_case("npu"))
                })
                .map_or(BackendSelection::TractCpu, |_| BackendSelection::OpenVino),
            BackendRequest::CoreMl => candidates
                .iter()
                .find(|r| {
                    r.is_promoted()
                        && (r.backend_name.eq_ignore_ascii_case("coreml")
                            || r.backend_name.eq_ignore_ascii_case("ane"))
                })
                .map_or(BackendSelection::TractCpu, |_| BackendSelection::CoreMl),
        }
    }

    #[must_use]
    pub fn resolve_request(
        &self,
        request: BackendRequest,
        report: Option<&CalibrationReport>,
    ) -> BackendSelection {
        report.map_or(BackendSelection::TractCpu, |rep| {
            self.resolve_candidates(request, std::slice::from_ref(rep))
        })
    }
}
