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

    /// Records a crash timestamp.
    /// In Phase 1 RED, always returns false so terminal state is not triggered.
    pub fn record_crash(&mut self, timestamp: Instant) -> bool {
        self.crash_timestamps.push(timestamp);
        self.consecutive_crashes += 1;
        // RED: do not trigger limit yet
        false
    }

    #[must_use]
    pub fn crashes_in_window(&self, now: Instant) -> usize {
        let cutoff = now.checked_sub(FIFTEEN_MINUTES).unwrap_or(now);
        self.crash_timestamps.iter().filter(|&&t| t >= cutoff).count()
    }

    #[must_use]
    pub fn next_backoff_delay(&self) -> Duration {
        if self.consecutive_crashes == 0 {
            return Duration::ZERO;
        }
        let index = (self.consecutive_crashes - 1).min(BACKOFF_SECONDS.len() - 1);
        Duration::from_secs(BACKOFF_SECONDS[index])
    }

    pub fn reset(&mut self) {
        self.crash_timestamps.clear();
        self.consecutive_crashes = 0;
    }
}
