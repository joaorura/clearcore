#![forbid(unsafe_code)]
#![allow(clippy::missing_errors_doc, clippy::missing_const_for_fn)]

use core::fmt;
use std::error::Error;
use std::sync::Arc;

use realtime_noise_contracts::RealtimeTransport;
use realtime_noise_model::InferenceBackend;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DenoiseMode {
    #[default]
    Active,
    Bypass,
    Mute,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResetReason {
    InferenceDeadlineMiss,
    QueueAgeWatermarkExceeded,
    DeviceChange,
    CaptureDrop,
    BackendChange,
    UserRequested,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EngineState {
    #[default]
    Stopped,
    Starting,
    WaitingForDevice,
    Running,
    Degraded,
    Stopping,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineStatus {
    pub state: EngineState,
    pub mode: DenoiseMode,
    pub generation: u64,
    pub deadline_miss_count: u64,
}

impl EngineStatus {
    #[must_use]
    pub const fn state(&self) -> EngineState {
        self.state
    }

    #[must_use]
    pub const fn mode(&self) -> DenoiseMode {
        self.mode
    }

    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    #[must_use]
    pub const fn deadline_miss_count(&self) -> u64 {
        self.deadline_miss_count
    }

    #[must_use]
    pub const fn is_running(&self) -> bool {
        matches!(self.state, EngineState::Running)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineError {
    NotImplemented,
    AlreadyRunning,
    NotRunning,
    BackendError(String),
    TransportError(String),
    InvalidConfiguration(String),
}

impl fmt::Display for EngineError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotImplemented => formatter.write_str("feature or method not implemented"),
            Self::AlreadyRunning => formatter.write_str("engine is already running"),
            Self::NotRunning => formatter.write_str("engine is not running"),
            Self::BackendError(msg) => write!(formatter, "backend error: {msg}"),
            Self::TransportError(msg) => write!(formatter, "transport error: {msg}"),
            Self::InvalidConfiguration(msg) => write!(formatter, "invalid configuration: {msg}"),
        }
    }
}

impl Error for EngineError {}

pub struct DenoiseEngine {
    input: Arc<dyn RealtimeTransport>,
    output: Arc<dyn RealtimeTransport>,
    backend: Option<Box<dyn InferenceBackend>>,
    mode: DenoiseMode,
    status: EngineStatus,
}

impl DenoiseEngine {
    #[must_use]
    pub fn new(
        input: Arc<dyn RealtimeTransport>,
        output: Arc<dyn RealtimeTransport>,
        backend: Box<dyn InferenceBackend>,
    ) -> Self {
        Self {
            input,
            output,
            backend: Some(backend),
            mode: DenoiseMode::Active,
            status: EngineStatus {
                state: EngineState::Stopped,
                mode: DenoiseMode::Active,
                generation: 1,
                deadline_miss_count: 0,
            },
        }
    }

    pub fn start(&mut self) -> Result<(), EngineError> {
        let _ = &self.input;
        let _ = &self.output;
        let _ = &self.backend;
        Err(EngineError::NotImplemented)
    }

    pub fn stop(&mut self) -> Result<(), EngineError> {
        Err(EngineError::NotImplemented)
    }

    pub fn set_mode(&mut self, mode: DenoiseMode) {
        self.mode = mode;
        self.status.mode = mode;
    }

    pub fn set_backend(&mut self, backend: Box<dyn InferenceBackend>) -> Result<(), EngineError> {
        self.backend = Some(backend);
        Err(EngineError::NotImplemented)
    }

    pub fn begin_generation_restart(&mut self, reason: ResetReason) -> Result<u64, EngineError> {
        let _ = reason;
        Err(EngineError::NotImplemented)
    }

    #[must_use]
    pub fn status(&self) -> EngineStatus {
        self.status.clone()
    }
}
