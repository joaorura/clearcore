#![forbid(unsafe_code)]

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

/// Requested backend mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum BackendRequest {
    #[default]
    Auto,
    TractCpu,
    Cuda,
    OpenVino,
    CoreMl,
}

/// Backend selected by the AUTO policy or direct request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum BackendSelection {
    #[default]
    TractCpu,
    Cuda,
    OpenVino,
    CoreMl,
}

impl BackendSelection {
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::TractCpu => "tract",
            Self::Cuda => "cuda",
            Self::OpenVino => "openvino",
            Self::CoreMl => "coreml",
        }
    }

    #[must_use]
    pub const fn is_tract_cpu(&self) -> bool {
        matches!(self, Self::TractCpu)
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

    match report.backend_name.to_ascii_lowercase().as_str() {
        "cuda" | "tensorrt" => BackendSelection::Cuda,
        "openvino" | "npu" => BackendSelection::OpenVino,
        "coreml" | "ane" => BackendSelection::CoreMl,
        _ => BackendSelection::TractCpu,
    }
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
    pub fn resolve_request(
        &self,
        request: BackendRequest,
        report: Option<&CalibrationReport>,
    ) -> BackendSelection {
        match request {
            BackendRequest::Auto => {
                report.map_or(BackendSelection::TractCpu, |rep| self.select(rep))
            }
            BackendRequest::TractCpu => BackendSelection::TractCpu,
            BackendRequest::Cuda => match report {
                Some(rep)
                    if rep.is_promoted() && rep.backend_name.eq_ignore_ascii_case("cuda") =>
                {
                    BackendSelection::Cuda
                }
                _ => BackendSelection::TractCpu,
            },
            BackendRequest::OpenVino => match report {
                Some(rep)
                    if rep.is_promoted() && rep.backend_name.eq_ignore_ascii_case("openvino") =>
                {
                    BackendSelection::OpenVino
                }
                _ => BackendSelection::TractCpu,
            },
            BackendRequest::CoreMl => match report {
                Some(rep)
                    if rep.is_promoted() && rep.backend_name.eq_ignore_ascii_case("coreml") =>
                {
                    BackendSelection::CoreMl
                }
                _ => BackendSelection::TractCpu,
            },
        }
    }
}
