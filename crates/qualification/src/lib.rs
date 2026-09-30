#![forbid(unsafe_code)]

/// Supported target platforms for simultaneous GA qualification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatformId {
    Windows11,
    MacOS13,
    Ubuntu2404,
    Fedora42,
}

/// Evaluation status for an individual platform qualification run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatformStatus {
    Passed,
    Failed,
    Blocked,
    Unverified,
}

impl PlatformStatus {
    #[must_use]
    pub const fn is_passed(self) -> bool {
        matches!(self, Self::Passed)
    }
}

/// Final authoritative decision for General Availability (GA).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GaDecision {
    Approved,
    Blocked,
}

impl GaDecision {
    #[must_use]
    pub const fn is_approved(self) -> bool {
        matches!(self, Self::Approved)
    }
}

/// Aggregates individual platform statuses into a single simultaneous GA decision.
///
/// Strictly enforces the rule: GA requires all 4 target platforms (Windows 11, macOS 13+,
/// Ubuntu 24.04 LTS, Fedora 42+) to pass simultaneously. Any single failure, block,
/// or unverified status produces `GaDecision::Blocked`.
#[must_use]
pub fn aggregate(statuses: &[PlatformStatus]) -> GaDecision {
    const REQUIRED_PLATFORM_COUNT: usize = 4;
    if statuses.len() == REQUIRED_PLATFORM_COUNT && statuses.iter().all(|s| s.is_passed()) {
        GaDecision::Approved
    } else {
        GaDecision::Blocked
    }
}

/// Metric thresholds mandated for each platform's qualification pass.
pub struct MetricThresholds {
    /// Maximum allowed product end-to-end latency p95 (80.0 ms).
    pub max_product_p95_ms: f64,
    /// Maximum allowed CPU inference latency p99 (7.0 ms).
    pub max_cpu_p99_ms: f64,
    /// Allowed soak test audio dropout events (must be 0).
    pub max_soak_dropouts: usize,
}

impl Default for MetricThresholds {
    fn default() -> Self {
        Self {
            max_product_p95_ms: 80.0,
            max_cpu_p99_ms: 7.0,
            max_soak_dropouts: 0,
        }
    }
}
