#![forbid(unsafe_code)]
#![allow(
    clippy::missing_errors_doc,
    clippy::missing_const_for_fn,
    clippy::significant_drop_tightening
)]

use core::fmt;
use std::error::Error;
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use realtime_noise_contracts::{AudioFrame, HOP_SAMPLES, RealtimeTransport};
use realtime_noise_model::{InferenceBackend, InferenceError};

use crate::generation::{Generation, GenerationId};
use crate::queue::BoundedQueueTransport;
use crate::worker::DenoiseWorker;

/// Hard deadline for inference processing per hop (10.0 ms @ 48 kHz).
pub const INFERENCE_HARD_DEADLINE: Duration = Duration::from_millis(10);

/// Operating mode of the real-time noise suppression engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DenoiseMode {
    #[default]
    Active,
    Bypass,
    Mute,
}

/// Reason for closing an active generation and initiating a restart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResetReason {
    InferenceDeadlineMiss,
    QueueAgeWatermarkExceeded,
    DeviceChange,
    CaptureDrop,
    BackendChange,
    UserRequested,
}

/// Lifecycle state of the engine coordinator.
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

/// Snapshot of current engine status and telemetry counters.
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

/// Engine operation error.
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

/// Shared internal state between the engine coordinator and worker thread.
pub(crate) struct EngineSharedState {
    pub(crate) state: EngineState,
    pub(crate) mode: DenoiseMode,
    pub(crate) backend: Option<Box<dyn InferenceBackend>>,
    pub(crate) pending_backend: Option<Box<dyn InferenceBackend>>,
    pub(crate) generation: Generation,
    pub(crate) deadline_miss_count: u64,
    pub(crate) is_running: bool,
}

/// Real-time noise suppression engine coordinator.
pub struct DenoiseEngine {
    input: Arc<dyn RealtimeTransport>,
    output: Arc<dyn RealtimeTransport>,
    shared: Arc<Mutex<EngineSharedState>>,
    worker_handle: Option<thread::JoinHandle<()>>,
}

impl DenoiseEngine {
    /// Creates a new `DenoiseEngine` with the given input and output transports and backend.
    #[must_use]
    pub fn new(
        input: Arc<dyn RealtimeTransport>,
        output: Arc<dyn RealtimeTransport>,
        backend: Box<dyn InferenceBackend>,
    ) -> Self {
        Self::with_mode(input, output, backend, DenoiseMode::Active)
    }

    /// Creates a new `DenoiseEngine` with the given transports, backend, and initial mode.
    #[must_use]
    pub fn with_mode(
        input: Arc<dyn RealtimeTransport>,
        output: Arc<dyn RealtimeTransport>,
        backend: Box<dyn InferenceBackend>,
        mode: DenoiseMode,
    ) -> Self {
        let initial_generation = Generation::active(GenerationId::new(1));
        let shared = Arc::new(Mutex::new(EngineSharedState {
            state: EngineState::Stopped,
            mode,
            backend: Some(backend),
            pending_backend: None,
            generation: initial_generation,
            deadline_miss_count: 0,
            is_running: false,
        }));

        Self {
            input,
            output,
            shared,
            worker_handle: None,
        }
    }

    /// Creates a new standalone `DenoiseEngine` with internal default queues.
    #[must_use]
    pub fn new_standalone(backend: Box<dyn InferenceBackend>, mode: DenoiseMode) -> Self {
        let input = Arc::new(BoundedQueueTransport::new());
        let output = Arc::new(BoundedQueueTransport::new());
        Self::with_mode(input, output, backend, mode)
    }

    /// Starts the engine and background worker thread.
    pub fn start(&mut self) -> Result<(), EngineError> {
        let mut shared = self.shared.lock().unwrap_or_else(PoisonError::into_inner);
        if shared.is_running {
            return Err(EngineError::AlreadyRunning);
        }

        shared.state = EngineState::Running;
        shared.is_running = true;
        shared.generation.mark_warmed();
        drop(shared);

        let shared_clone = Arc::clone(&self.shared);
        let input_clone = Arc::clone(&self.input);
        let output_clone = Arc::clone(&self.output);

        let handle = thread::Builder::new()
            .name("realtime-noise-worker".to_owned())
            .spawn(move || {
                DenoiseWorker::run_loop(&shared_clone, &input_clone, &output_clone);
            })
            .map_err(|e| EngineError::InvalidConfiguration(e.to_string()))?;

        self.worker_handle = Some(handle);
        Ok(())
    }

    /// Stops the engine and joins the background worker thread.
    pub fn stop(&mut self) -> Result<(), EngineError> {
        let mut shared = self.shared.lock().unwrap_or_else(PoisonError::into_inner);
        if !shared.is_running {
            return Err(EngineError::NotRunning);
        }

        shared.is_running = false;
        shared.state = EngineState::Stopped;
        drop(shared);

        if let Some(handle) = self.worker_handle.take() {
            let _ = handle.join();
        }

        Ok(())
    }

    /// Sets the denoise mode.
    pub fn set_mode(&mut self, mode: DenoiseMode) {
        let mut shared = self.shared.lock().unwrap_or_else(PoisonError::into_inner);
        shared.mode = mode;
    }

    /// Safely replaces the inference backend on the next hop boundary.
    pub fn set_backend(&mut self, backend: Box<dyn InferenceBackend>) -> Result<(), EngineError> {
        let mut shared = self.shared.lock().unwrap_or_else(PoisonError::into_inner);
        if shared.is_running {
            shared.pending_backend = Some(backend);
        } else {
            shared.backend = Some(backend);
        }
        drop(shared);
        Ok(())
    }

    /// Closes the current generation, increments generation ID, and records restart reason.
    pub fn begin_generation_restart(&mut self, reason: ResetReason) -> Result<u64, EngineError> {
        let mut shared = self.shared.lock().unwrap_or_else(PoisonError::into_inner);
        let old_id = shared.generation.id().get();
        shared.generation.close(reason);
        let next_id = shared.generation.id().next();
        shared.generation = Generation::active(next_id);
        drop(shared);

        self.output.close_generation(old_id);
        Ok(next_id.get())
    }

    /// Returns a snapshot of the current engine status.
    #[must_use]
    pub fn status(&self) -> EngineStatus {
        let shared = self.shared.lock().unwrap_or_else(PoisonError::into_inner);
        EngineStatus {
            state: shared.state,
            mode: shared.mode,
            generation: shared.generation.id().get(),
            deadline_miss_count: shared.deadline_miss_count,
        }
    }

    /// Synchronously processes a single audio frame following the engine mode and deadline policy.
    pub fn process_frame(&mut self, input: &AudioFrame) -> Result<AudioFrame, EngineError> {
        let mut shared = self.shared.lock().unwrap_or_else(PoisonError::into_inner);
        if !shared.is_running {
            return Err(EngineError::NotRunning);
        }

        let mode = shared.mode;
        match mode {
            DenoiseMode::Mute => {
                drop(shared);
                Ok([0.0; HOP_SAMPLES])
            }
            DenoiseMode::Bypass => {
                drop(shared);
                Ok(*input)
            }
            DenoiseMode::Active => {
                let start = Instant::now();
                let process_result = shared.backend.as_mut().map_or_else(
                    || {
                        Err(InferenceError::UnsupportedCpuProfile(
                            "no backend available".to_owned(),
                        ))
                    },
                    |backend| backend.process(input),
                );
                let elapsed = start.elapsed();

                if elapsed > INFERENCE_HARD_DEADLINE || process_result.is_err() {
                    shared.deadline_miss_count = shared.deadline_miss_count.saturating_add(1);
                    shared.generation.close(ResetReason::InferenceDeadlineMiss);
                    let next_id = shared.generation.id().next();
                    shared.generation = Generation::active(next_id);
                    drop(shared);
                    Ok([0.0; HOP_SAMPLES])
                } else if let Ok(processed) = process_result {
                    drop(shared);
                    Ok(processed.samples)
                } else {
                    drop(shared);
                    Ok([0.0; HOP_SAMPLES])
                }
            }
        }
    }
}

impl Drop for DenoiseEngine {
    fn drop(&mut self) {
        if self.worker_handle.is_some() {
            let _ = self.stop();
        }
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;
    use realtime_noise_model::{BackendDescriptor, ProcessedFrame};

    struct PassthroughBackend {
        descriptor: BackendDescriptor,
    }

    impl PassthroughBackend {
        fn new() -> Self {
            Self {
                descriptor: BackendDescriptor {
                    backend: "passthrough",
                    backend_version: "1.0",
                    runtime: "test",
                    runtime_version: "1.0",
                    asset_id: "test".to_owned(),
                    asset_sha256: "0".repeat(64),
                    cpu_profile: "test",
                },
            }
        }
    }

    impl InferenceBackend for PassthroughBackend {
        fn descriptor(&self) -> BackendDescriptor {
            self.descriptor.clone()
        }

        fn process(&mut self, input: &AudioFrame) -> Result<ProcessedFrame, InferenceError> {
            ProcessedFrame::checked(*input, 0, self.descriptor.clone())
        }

        fn algorithmic_latency_samples(&self) -> u32 {
            0
        }
    }

    #[test]
    fn engine_standalone_process_frame_in_mute_mode() {
        let backend = Box::new(PassthroughBackend::new());
        let mut engine = DenoiseEngine::new_standalone(backend, DenoiseMode::Mute);
        assert_eq!(engine.status().state(), EngineState::Stopped);
        assert!(engine.start().is_ok());
        assert_eq!(engine.status().state(), EngineState::Running);

        let input_samples = [0.8; HOP_SAMPLES];
        let output = engine.process_frame(&input_samples);
        assert!(output.is_ok());
        match output {
            Ok(samples) => assert_eq!(samples, [0.0; HOP_SAMPLES]),
            Err(_) => unreachable!(),
        }

        assert!(engine.stop().is_ok());
        assert_eq!(engine.status().state(), EngineState::Stopped);
    }

    #[test]
    fn engine_lifecycle_start_stop() {
        let backend = Box::new(PassthroughBackend::new());
        let mut engine = DenoiseEngine::new_standalone(backend, DenoiseMode::Active);
        assert_eq!(engine.start(), Ok(()));
        assert_eq!(engine.start(), Err(EngineError::AlreadyRunning));
        assert_eq!(engine.stop(), Ok(()));
        assert_eq!(engine.stop(), Err(EngineError::NotRunning));
    }

    #[test]
    fn engine_restart_increments_generation() {
        let backend = Box::new(PassthroughBackend::new());
        let mut engine = DenoiseEngine::new_standalone(backend, DenoiseMode::Active);
        assert_eq!(engine.status().generation(), 1);

        let new_gen = engine.begin_generation_restart(ResetReason::UserRequested);
        assert_eq!(new_gen, Ok(2));
        assert_eq!(engine.status().generation(), 2);
    }
}
