#![forbid(unsafe_code)]

use std::time::Instant;
use realtime_noise_engine::DenoiseMode;
use crate::backoff::BackoffTracker;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SupervisorState {
    Running,
    EngineUnavailable,
    Restarting { attempt: usize, next_retry_ms: u64 },
    TerminalSafeState { reason: String, diagnostic: Option<String> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupervisorStatus {
    pub state: SupervisorState,
    pub crash_count_15m: usize,
    pub total_crashes: u64,
    pub active_mode: DenoiseMode,
}

pub struct EngineSupervisor {
    state: SupervisorState,
    mode: DenoiseMode,
    backoff: BackoffTracker,
    total_crashes: u64,
}

impl Default for EngineSupervisor {
    fn default() -> Self {
        Self::new(DenoiseMode::Active)
    }
}

impl EngineSupervisor {
    #[must_use]
    pub const fn new(initial_mode: DenoiseMode) -> Self {
        Self {
            state: SupervisorState::Running,
            mode: initial_mode,
            backoff: BackoffTracker::new(),
            total_crashes: 0,
        }
    }

    #[must_use]
    pub const fn state(&self) -> &SupervisorState {
        &self.state
    }

    #[must_use]
    pub fn status(&self) -> SupervisorStatus {
        SupervisorStatus {
            state: self.state.clone(),
            crash_count_15m: self.backoff.crashes_in_window(Instant::now()),
            total_crashes: self.total_crashes,
            active_mode: self.mode,
        }
    }

    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        matches!(self.state, SupervisorState::TerminalSafeState { .. })
    }

    #[must_use]
    pub const fn can_restart(&self) -> bool {
        !self.is_terminal()
    }

    pub fn record_crash(&mut self, _reason: &str, timestamp: Instant) {
        self.total_crashes += 1;
        let limit_exceeded = self.backoff.record_crash(timestamp);
        // SKELETON (RED): intentionally ignore limit_exceeded for now
        if limit_exceeded {
            self.state = SupervisorState::TerminalSafeState {
                reason: "Exceeded crash limit (5 crashes per 15 minutes)".to_string(),
                diagnostic: Some(_reason.to_string()),
            };
        } else {
            self.state = SupervisorState::Running;
        }
    }

    pub fn try_restart(&mut self) -> Result<(), &'static str> {
        if self.is_terminal() {
            Err("Cannot restart: engine is in terminal safe state")
        } else {
            self.state = SupervisorState::Running;
            Ok(())
        }
    }

    pub fn reset(&mut self) -> Result<(), &'static str> {
        self.state = SupervisorState::Running;
        self.backoff.reset();
        Ok(())
    }

    pub fn set_mode(&mut self, mode: DenoiseMode) {
        self.mode = mode;
    }
}
