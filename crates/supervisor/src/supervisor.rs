#![forbid(unsafe_code)]

use crate::backoff::{BackoffTracker, MAX_CRASHES_PER_15_MINUTES};
use realtime_noise_engine::DenoiseMode;
use std::fmt;
use std::time::Instant;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SupervisorState {
    Running,
    EngineUnavailable,
    Restarting {
        attempt: usize,
        next_retry_ms: u64,
    },
    TerminalSafeState {
        reason: String,
        diagnostic: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupervisorStatus {
    pub state: SupervisorState,
    pub crash_count_15m: usize,
    pub total_crashes: u64,
    pub active_mode: DenoiseMode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SupervisorError {
    TerminalSafeState(String),
    EngineUnavailable(String),
    InvalidTransition(String),
}

impl fmt::Display for SupervisorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TerminalSafeState(msg) => write!(f, "Terminal safe state: {msg}"),
            Self::EngineUnavailable(msg) => write!(f, "Engine unavailable: {msg}"),
            Self::InvalidTransition(msg) => write!(f, "Invalid state transition: {msg}"),
        }
    }
}

impl std::error::Error for SupervisorError {}

pub struct EngineSupervisor {
    state: SupervisorState,
    mode: DenoiseMode,
    backoff: BackoffTracker,
    total_crashes: u64,
    diagnostics_log: Vec<String>,
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
            diagnostics_log: Vec::new(),
        }
    }

    #[must_use]
    pub const fn state(&self) -> &SupervisorState {
        &self.state
    }

    #[must_use]
    pub const fn mode(&self) -> DenoiseMode {
        self.mode
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
    pub const fn is_running(&self) -> bool {
        matches!(self.state, SupervisorState::Running)
    }

    #[must_use]
    pub const fn can_restart(&self) -> bool {
        !self.is_terminal()
    }

    #[must_use]
    pub fn diagnostics(&self) -> &[String] {
        &self.diagnostics_log
    }

    /// Records an engine crash, logs diagnostics, and calculates backoff or terminal safe state.
    pub fn record_crash(&mut self, reason: &str, timestamp: Instant) {
        self.total_crashes += 1;
        let log_entry = format!("Crash #{}: {}", self.total_crashes, reason);
        if self.diagnostics_log.len() >= 100 {
            self.diagnostics_log.remove(0);
        }
        self.diagnostics_log.push(log_entry);

        let limit_exceeded = self.backoff.record_crash(timestamp);
        if limit_exceeded {
            self.state = SupervisorState::TerminalSafeState {
                reason: format!(
                    "Exceeded crash limit ({MAX_CRASHES_PER_15_MINUTES} crashes per 15 minutes)"
                ),
                diagnostic: Some(reason.to_string()),
            };
        } else {
            let attempt = self.backoff.consecutive_crashes();
            let delay = self.backoff.next_backoff_delay();
            #[allow(clippy::cast_possible_truncation)]
            let next_retry_ms = delay.as_millis() as u64;
            self.state = SupervisorState::Restarting {
                attempt,
                next_retry_ms,
            };
        }
    }

    /// Attempts to restart the engine. Fails if supervisor is in `TerminalSafeState`.
    pub fn try_restart(&mut self) -> Result<(), SupervisorError> {
        if let SupervisorState::TerminalSafeState { reason, .. } = &self.state {
            return Err(SupervisorError::TerminalSafeState(format!(
                "Cannot restart while in terminal safe state ({reason}). Explicit user reset required."
            )));
        }

        self.state = SupervisorState::Running;
        Ok(())
    }

    /// User-initiated reset to clear `TerminalSafeState` and restore `Running` state.
    pub fn reset(&mut self) -> Result<(), SupervisorError> {
        self.state = SupervisorState::Running;
        self.backoff.reset();
        self.diagnostics_log
            .push("Explicit supervisor reset initiated by control client".to_string());
        Ok(())
    }

    /// Sets the active denoise operating mode.
    pub fn set_mode(&mut self, mode: DenoiseMode) {
        self.mode = mode;
    }

    /// Enforces digital silence if engine is not running or in terminal safe state.
    pub fn process_frame_or_silence(&self, frame: &mut [f32]) {
        if !self.is_running() || self.is_terminal() || self.mode == DenoiseMode::Mute {
            frame.fill(0.0);
        }
    }
}

/// Converts IPC `DenoiseMode` to engine `DenoiseMode`.
#[must_use]
pub const fn convert_ipc_mode_to_engine(mode: realtime_noise_ipc::DenoiseMode) -> DenoiseMode {
    match mode {
        realtime_noise_ipc::DenoiseMode::Active => DenoiseMode::Active,
        realtime_noise_ipc::DenoiseMode::Bypass => DenoiseMode::Bypass,
        realtime_noise_ipc::DenoiseMode::Mute => DenoiseMode::Mute,
    }
}

/// Converts engine `DenoiseMode` to IPC `DenoiseMode`.
#[must_use]
pub const fn convert_engine_mode_to_ipc(mode: DenoiseMode) -> realtime_noise_ipc::DenoiseMode {
    match mode {
        DenoiseMode::Active => realtime_noise_ipc::DenoiseMode::Active,
        DenoiseMode::Bypass => realtime_noise_ipc::DenoiseMode::Bypass,
        DenoiseMode::Mute => realtime_noise_ipc::DenoiseMode::Mute,
    }
}
