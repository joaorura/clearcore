//! Neural half of the filter: the [`Engine`] trait implemented by every runtime (ONNX Runtime on
//! the CPU, OpenVINO on NPU/GPU/CPU), and [`FallbackEngine`], which runs the selected engine and
//! swaps to the ONNX Runtime standby when the selected one fails or overruns the hop budget.
//!
//! Every engine runs **all** of its graphs on **every** hop and advances every recurrent state
//! (decision of 2026-11-09, faithful to training): the local SNR only decides which stages the
//! DSP pipeline *applies*, never which graphs run.

use std::time::{Duration, Instant};

use realtime_noise_model::dsp_pipeline::{DEFAULT_NB_DF, DEFAULT_NB_ERB};
use realtime_noise_model::{
    BackendDescriptor, ExtractedFeatures, FiLMVectors, InferenceError, ModelFrameOutput,
    SpectralModelBackend,
};

/// Width of the speaker embedding consumed by pDFNet3 Pro v3 (ECAPA, L2-normalised).
pub const SPEAKER_EMBEDDING_DIM: usize = 192;

/// Speaker conditioning handed to the engines. Each engine uses the part its graphs understand:
/// the v3 graph takes `embedding` (+ `speaker_present`), the v2 graphs take `film`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Conditioning {
    /// L2-normalised speaker embedding (`SPEAKER_EMBEDDING_DIM` values). `None` = no speaker:
    /// zeros and `speaker_present = 0`.
    pub embedding: Option<Vec<f32>>,
    /// `FiLM` vectors for the v2 graphs. `None` = identity.
    pub film: Option<FiLMVectors>,
}

/// One runtime executing the neural graphs for one hop (`S = 1`).
pub trait Engine: Send {
    /// Short label, e.g. `ort:CPU` or `openvino:NPU`.
    fn label(&self) -> String;
    /// Runs every graph for one hop and commits every state only on success.
    fn run(&mut self, features: &ExtractedFeatures) -> Result<ModelFrameOutput, String>;
    /// Zeroes every recurrent state.
    fn reset(&mut self) -> Result<(), String>;
    /// Replaces the speaker conditioning between two hops (state untouched).
    fn set_conditioning(&mut self, conditioning: &Conditioning) -> Result<(), String>;
    /// Whether the graphs consume a speaker (`speaker_embedding` or `FiLM`).
    fn supports_speaker(&self) -> bool;
    /// Committed recurrent state, in a runtime-independent order, when the engine can export it.
    fn export_state(&self) -> Option<Vec<Vec<f32>>> {
        None
    }
    /// Loads a state exported by [`Engine::export_state`] of an engine of the same model.
    fn import_state(&mut self, _state: &[Vec<f32>]) -> Result<(), String> {
        Err("state import not supported".to_owned())
    }
}

/// Real-time budget of one hop (480 samples at 48 kHz).
pub const HOP_BUDGET: Duration = Duration::from_millis(10);
/// Environment override of the hop budget, in microseconds (tests and diagnostics).
pub const ENV_HOP_BUDGET_US: &str = "CLEARCORE_FILTER_HOP_BUDGET_US";

/// Overrun policy of the selected (non-standby) engine.
///
/// - a hop above `hard_factor * budget` after the first `grace_hops` swaps immediately;
/// - `max_overruns` hops above `budget` inside the last `window` hops swap as well.
#[derive(Debug, Clone, Copy)]
pub struct LatencyPolicy {
    pub budget: Duration,
    pub hard_factor: u32,
    pub grace_hops: u64,
    pub window: usize,
    pub max_overruns: usize,
}

impl LatencyPolicy {
    #[must_use]
    pub fn from_env() -> Self {
        let budget = std::env::var(ENV_HOP_BUDGET_US)
            .ok()
            .and_then(|v| v.trim().parse::<u64>().ok())
            .filter(|us| *us > 0)
            .map_or(HOP_BUDGET, Duration::from_micros);
        Self {
            budget,
            hard_factor: 3,
            grace_hops: 20,
            window: 500,
            max_overruns: 5,
        }
    }
}

/// Rolling per-hop latency record (last 4096 hops) with the overrun window of [`LatencyPolicy`].
#[derive(Debug, Clone)]
pub struct LatencyMonitor {
    policy: LatencyPolicy,
    samples: Vec<u32>,
    next: usize,
    hops: u64,
    overrun_hops: std::collections::VecDeque<u64>,
}

const HISTORY: usize = 4096;

/// p50 / p99 / max of the recorded hops, in microseconds.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LatencyStats {
    pub hops: u64,
    pub p50_us: u32,
    pub p99_us: u32,
    pub max_us: u32,
}

impl LatencyMonitor {
    #[must_use]
    pub fn new(policy: LatencyPolicy) -> Self {
        Self {
            policy,
            samples: Vec::with_capacity(HISTORY),
            next: 0,
            hops: 0,
            overrun_hops: std::collections::VecDeque::new(),
        }
    }

    /// Records one hop; returns the reason to abandon the engine, if the policy says so.
    pub fn record(&mut self, elapsed: Duration) -> Option<String> {
        let us = u32::try_from(elapsed.as_micros()).unwrap_or(u32::MAX);
        if self.samples.len() < HISTORY {
            self.samples.push(us);
        } else {
            self.samples[self.next] = us;
        }
        self.next = (self.next + 1) % HISTORY;
        self.hops += 1;
        let budget = self.policy.budget;
        if self.hops <= self.policy.grace_hops {
            return None;
        }
        if elapsed > budget * self.policy.hard_factor {
            return Some(format!(
                "hop took {} us (> {}x the {} us budget)",
                elapsed.as_micros(),
                self.policy.hard_factor,
                budget.as_micros()
            ));
        }
        if elapsed > budget {
            self.overrun_hops.push_back(self.hops);
        }
        let window = self.policy.window as u64;
        while self
            .overrun_hops
            .front()
            .is_some_and(|&hop| hop + window <= self.hops)
        {
            self.overrun_hops.pop_front();
        }
        (self.overrun_hops.len() >= self.policy.max_overruns).then(|| {
            format!(
                "{} hops above the {} us budget in the last {} hops",
                self.overrun_hops.len(),
                budget.as_micros(),
                self.policy.window
            )
        })
    }

    #[must_use]
    pub fn stats(&self) -> LatencyStats {
        latency_stats(&self.samples, self.hops)
    }

    /// Forgets the recorded hops (after a swap the standby starts a fresh record).
    pub fn clear(&mut self) {
        self.samples.clear();
        self.next = 0;
        self.hops = 0;
        self.overrun_hops.clear();
    }
}

/// p50 / p99 / max of `samples` (microseconds).
#[must_use]
pub fn latency_stats(samples: &[u32], hops: u64) -> LatencyStats {
    if samples.is_empty() {
        return LatencyStats::default();
    }
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    let pick = |q: f64| {
        let max_index = sorted.len() - 1;
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::cast_precision_loss
        )]
        let index = ((max_index as f64) * q).round() as usize;
        sorted[index.min(max_index)]
    };
    LatencyStats {
        hops,
        p50_us: pick(0.50),
        p99_us: pick(0.99),
        max_us: sorted[sorted.len() - 1],
    }
}

/// Zero features for qualification hops.
#[must_use]
pub fn zero_features() -> ExtractedFeatures {
    ExtractedFeatures {
        feat_erb: [0.0; DEFAULT_NB_ERB],
        feat_spec: [[0.0; DEFAULT_NB_DF]; 2],
        spec_noisy: Vec::new(),
    }
}

/// Real-time qualification of a freshly compiled engine: `warmup` hops, then `measured` hops;
/// passes when the slowest measured hop stays below 90 % of `budget`. The state is zeroed again
/// afterwards. Returns the measured statistics either way.
pub fn qualify(
    engine: &mut dyn Engine,
    budget: Duration,
    warmup: usize,
    measured: usize,
) -> Result<LatencyStats, (String, LatencyStats)> {
    let features = zero_features();
    for _ in 0..warmup {
        engine
            .run(&features)
            .map_err(|e| (format!("warm-up hop failed: {e}"), LatencyStats::default()))?;
    }
    let mut samples = Vec::with_capacity(measured);
    for _ in 0..measured {
        let start = Instant::now();
        engine.run(&features).map_err(|e| {
            (
                format!("qualification hop failed: {e}"),
                LatencyStats::default(),
            )
        })?;
        samples.push(u32::try_from(start.elapsed().as_micros()).unwrap_or(u32::MAX));
    }
    let stats = latency_stats(&samples, measured as u64);
    engine
        .reset()
        .map_err(|e| (format!("reset after qualification failed: {e}"), stats))?;
    let limit = budget.as_micros() * 9 / 10;
    if u128::from(stats.max_us) < limit {
        Ok(stats)
    } else {
        Err((
            format!(
                "slowest qualification hop {} us >= {} us (90% of the hop budget)",
                stats.max_us, limit
            ),
            stats,
        ))
    }
}

/// Runs the selected engine; on an error or an overrun (see [`LatencyPolicy`]) it moves to the
/// ONNX Runtime standby for the rest of the session, carrying the recurrent state over when both
/// engines can exchange it (v3) and zeroing it otherwise (v2 on OpenVINO).
pub struct FallbackEngine {
    descriptor: BackendDescriptor,
    active: Box<dyn Engine>,
    standby: Option<Box<dyn Engine>>,
    conditioning: Conditioning,
    monitor: LatencyMonitor,
    fallback_reason: Option<String>,
    /// Label of the engine that was abandoned, for diagnostics.
    abandoned: Option<String>,
}

impl FallbackEngine {
    #[must_use]
    pub fn new(
        descriptor: BackendDescriptor,
        active: Box<dyn Engine>,
        standby: Option<Box<dyn Engine>>,
        policy: LatencyPolicy,
    ) -> Self {
        Self {
            descriptor,
            active,
            standby,
            conditioning: Conditioning::default(),
            monitor: LatencyMonitor::new(policy),
            fallback_reason: None,
            abandoned: None,
        }
    }

    /// Records why the selected backend was not used at all (e.g. OpenVINO not found).
    pub fn note_selection_fallback(&mut self, reason: String) {
        self.fallback_reason = Some(reason);
    }

    #[must_use]
    pub fn label(&self) -> String {
        self.active.label()
    }

    #[must_use]
    pub fn fallback_reason(&self) -> Option<&str> {
        self.fallback_reason.as_deref()
    }

    #[must_use]
    pub fn abandoned(&self) -> Option<&str> {
        self.abandoned.as_deref()
    }

    #[must_use]
    pub fn export_state(&self) -> Option<Vec<Vec<f32>>> {
        self.active.export_state()
    }

    #[must_use]
    pub fn has_standby(&self) -> bool {
        self.standby.is_some()
    }

    #[must_use]
    pub fn latency(&self) -> LatencyStats {
        self.monitor.stats()
    }

    #[must_use]
    pub fn supports_speaker(&self) -> bool {
        self.active.supports_speaker()
    }

    /// Applies `conditioning` to the active engine (and remembers it for a later swap).
    pub fn set_conditioning(&mut self, conditioning: Conditioning) -> Result<(), String> {
        self.active.set_conditioning(&conditioning)?;
        self.conditioning = conditioning;
        Ok(())
    }

    /// Forces the swap to the standby (tests and diagnostics). Returns `false` without standby.
    pub fn force_fallback(&mut self, reason: &str) -> bool {
        self.swap(reason.to_owned())
    }

    fn swap(&mut self, reason: String) -> bool {
        let Some(mut standby) = self.standby.take() else {
            return false;
        };
        if standby.set_conditioning(&self.conditioning).is_err() {
            // Unreachable in practice (validated on the active engine); keep running neutral.
            let _ = standby.set_conditioning(&Conditioning::default());
        }
        let carried = self
            .active
            .export_state()
            .is_some_and(|state| standby.import_state(&state).is_ok());
        if !carried {
            let _ = standby.reset();
        }
        let old = std::mem::replace(&mut self.active, standby);
        self.abandoned = Some(old.label());
        self.fallback_reason = Some(format!("{}: {reason}", old.label()));
        self.monitor.clear();
        true
    }
}

impl SpectralModelBackend for FallbackEngine {
    fn descriptor(&self) -> BackendDescriptor {
        self.descriptor.clone()
    }

    fn process_spectrum(
        &mut self,
        features: &ExtractedFeatures,
    ) -> Result<ModelFrameOutput, InferenceError> {
        let start = Instant::now();
        match self.active.run(features) {
            Ok(output) => {
                let elapsed = start.elapsed();
                if self.standby.is_some()
                    && let Some(reason) = self.monitor.record(elapsed)
                {
                    // The hop succeeded and its state is committed: swap after it.
                    self.swap(reason);
                } else if self.standby.is_none() {
                    let _ = self.monitor.record(elapsed);
                }
                Ok(output)
            }
            Err(error) => {
                if self.swap(format!("inference failed: {error}")) {
                    self.active
                        .run(features)
                        .map_err(InferenceError::InferenceExecution)
                } else {
                    Err(InferenceError::InferenceExecution(error))
                }
            }
        }
    }

    fn reset(&mut self) -> Result<(), InferenceError> {
        self.active
            .reset()
            .map_err(InferenceError::InferenceExecution)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy() -> LatencyPolicy {
        LatencyPolicy {
            budget: Duration::from_millis(10),
            hard_factor: 3,
            grace_hops: 2,
            window: 10,
            max_overruns: 3,
        }
    }

    #[test]
    fn a_single_huge_hop_after_the_grace_period_trips_the_monitor() {
        let mut m = LatencyMonitor::new(policy());
        assert!(m.record(Duration::from_millis(50)).is_none(), "grace");
        assert!(m.record(Duration::from_millis(1)).is_none());
        assert!(m.record(Duration::from_millis(31)).is_some());
    }

    #[test]
    fn repeated_overruns_inside_the_window_trip_the_monitor_and_old_ones_expire() {
        let mut m = LatencyMonitor::new(policy());
        for _ in 0..2 {
            m.record(Duration::from_millis(1));
        }
        assert!(m.record(Duration::from_millis(12)).is_none());
        assert!(m.record(Duration::from_millis(12)).is_none());
        for _ in 0..10 {
            assert!(m.record(Duration::from_millis(1)).is_none());
        }
        // The two old overruns left the window: two more are still below the limit of three.
        assert!(m.record(Duration::from_millis(12)).is_none());
        assert!(m.record(Duration::from_millis(12)).is_none());
        assert!(m.record(Duration::from_millis(12)).is_some());
    }

    #[test]
    fn stats_are_percentiles_of_the_record() {
        let samples: Vec<u32> = (1..=100).collect();
        let s = latency_stats(&samples, 100);
        assert_eq!((s.p50_us, s.p99_us, s.max_us), (51, 99, 100));
    }
}
