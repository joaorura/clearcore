#![forbid(unsafe_code)]

use std::time::{Duration, Instant};

pub const BACKOFF_SECONDS: [u64; 5] = [1, 2, 4, 8, 16];
pub const MAX_CRASHES_PER_15_MINUTES: usize = 5;
pub const FIFTEEN_MINUTES: Duration = Duration::from_secs(15 * 60);

#[derive(Debug, Clone, Default)]
pub struct BackoffTracker {
    crash_timestamps: Vec<Instant>,
    consecutive_crashes: usize,
}

impl BackoffTracker {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            crash_timestamps: Vec::new(),
            consecutive_crashes: 0,
        }
    }

    /// Prunes timestamps older than 15 minutes before the given timestamp.
    fn prune_old_crashes(&mut self, now: Instant) {
        if let Some(cutoff) = now.checked_sub(FIFTEEN_MINUTES) {
            self.crash_timestamps.retain(|&ts| ts >= cutoff);
        }
    }

    /// Records a crash timestamp and returns `true` if the crash limit
    /// (strictly greater than 5 crashes within 15 minutes) is exceeded.
    pub fn record_crash(&mut self, timestamp: Instant) -> bool {
        self.prune_old_crashes(timestamp);
        self.crash_timestamps.push(timestamp);
        self.consecutive_crashes += 1;

        self.crash_timestamps.len() > MAX_CRASHES_PER_15_MINUTES
    }

    #[must_use]
    pub const fn consecutive_crashes(&self) -> usize {
        self.consecutive_crashes
    }

    #[must_use]
    pub fn crashes_in_window(&self, now: Instant) -> usize {
        let cutoff = now.checked_sub(FIFTEEN_MINUTES).unwrap_or(now);
        self.crash_timestamps
            .iter()
            .filter(|&&t| t >= cutoff)
            .count()
    }

    #[must_use]
    pub fn next_backoff_delay(&self) -> Duration {
        if self.consecutive_crashes == 0 {
            return Duration::ZERO;
        }
        let index = (self.consecutive_crashes - 1).min(BACKOFF_SECONDS.len() - 1);
        Duration::from_secs(BACKOFF_SECONDS[index])
    }

    /// Called after an extended period of healthy execution to reset backoff step
    /// while retaining crash history within the sliding window.
    pub fn on_healthy_period(&mut self) {
        self.consecutive_crashes = 0;
    }

    /// Full manual reset (e.g. user-initiated reset command via IPC).
    pub fn reset(&mut self) {
        self.crash_timestamps.clear();
        self.consecutive_crashes = 0;
    }
}
