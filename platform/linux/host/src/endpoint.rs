#![forbid(unsafe_code)]
#![allow(clippy::missing_errors_doc, clippy::missing_const_for_fn)]

use realtime_noise_contracts::{
    AudioFrame, EndpointError, EndpointStatus, HOP_SAMPLES, VirtualMicrophone,
};
use realtime_noise_engine::{DenoiseMode, GenerationId, GenerationState};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Engine supervisor state mirrored for endpoint gating and fail-closed silence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SupervisorState {
    Running,
    EngineUnavailable,
    Restarting,
    TerminalSafeState,
}

/// Represents a downstream audio consumer stream (e.g. Teams, Zoom, Discord, OBS, WebRTC).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsumerStream {
    pub id: String,
    pub bound_node_id: Option<u32>,
    pub needs_rebind: bool,
}

/// Configuration for supervising the per-user native `PipeWire` helper process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HelperProcessConfig {
    pub binary_path: PathBuf,
    pub node_name: String,
    pub node_description: String,
    pub lock_path: PathBuf,
}

impl Default for HelperProcessConfig {
    fn default() -> Self {
        let lock_dir =
            std::env::var("XDG_RUNTIME_DIR").map_or_else(|_| PathBuf::from("/tmp"), PathBuf::from);
        Self {
            binary_path: PathBuf::from("platform/linux/helper/build/pipewire_helper"),
            node_name: "realtime-noise-source".to_string(),
            node_description: "Realtime Noise Virtual Microphone".to_string(),
            lock_path: lock_dir.join("hippocamp_pipewire_helper.lock"),
        }
    }
}

/// Linux `PipeWire` Virtual Microphone Host Endpoint.
///
/// Supervises the per-user `pipewire_helper` process, manages `PipeWire` node
/// recreation and consumer stream rebind semantics, tracks generations,
/// and enforces fail-closed digital silence policy on engine failure, backoff, or restart.
#[derive(Debug)]
pub struct LinuxVirtualMicrophone {
    config: HelperProcessConfig,
    status: EndpointStatus,
    current_node_id: u32,
    active_generation: GenerationId,
    generation_state: GenerationState,
    supervisor_state: SupervisorState,
    denoise_mode: DenoiseMode,
    is_contended: bool,
    latest_frame: AudioFrame,
    consumers: HashMap<String, ConsumerStream>,
}

impl Default for LinuxVirtualMicrophone {
    fn default() -> Self {
        Self::new()
    }
}

impl LinuxVirtualMicrophone {
    #[must_use]
    pub fn new() -> Self {
        Self::with_config(HelperProcessConfig::default())
    }

    #[must_use]
    pub fn with_config(config: HelperProcessConfig) -> Self {
        Self {
            config,
            status: EndpointStatus::Silence,
            current_node_id: 100,
            active_generation: GenerationId::new(1),
            generation_state: GenerationState::Warming,
            supervisor_state: SupervisorState::Running,
            denoise_mode: DenoiseMode::Active,
            is_contended: false,
            latest_frame: [0.0; HOP_SAMPLES],
            consumers: HashMap::new(),
        }
    }

    #[must_use]
    pub fn config(&self) -> &HelperProcessConfig {
        &self.config
    }

    #[must_use]
    pub fn current_node_id(&self) -> u32 {
        self.current_node_id
    }

    #[must_use]
    pub fn active_generation(&self) -> GenerationId {
        self.active_generation
    }

    #[must_use]
    pub fn generation_state(&self) -> GenerationState {
        self.generation_state
    }

    #[must_use]
    pub fn supervisor_state(&self) -> SupervisorState {
        self.supervisor_state
    }

    #[must_use]
    pub fn denoise_mode(&self) -> DenoiseMode {
        self.denoise_mode
    }

    #[must_use]
    pub fn is_contended(&self) -> bool {
        self.is_contended
    }

    pub fn set_supervisor_state(&mut self, state: SupervisorState) {
        self.supervisor_state = state;
        if state != SupervisorState::Running && self.status == EndpointStatus::Active {
            self.status = EndpointStatus::Silence;
        }
    }

    pub fn set_denoise_mode(&mut self, mode: DenoiseMode) {
        self.denoise_mode = mode;
    }

    pub fn set_contention(&mut self, contended: bool) {
        self.is_contended = contended;
        if contended {
            self.status = EndpointStatus::UnavailableBusy;
        }
    }

    pub fn set_generation(&mut self, id: GenerationId, state: GenerationState) {
        self.active_generation = id;
        self.generation_state = state;
    }

    pub fn warm_generation(&mut self) {
        self.generation_state = GenerationState::Active;
    }

    pub fn push_frame(&mut self, frame: AudioFrame) {
        self.latest_frame = frame;
    }

    /// Registers a downstream consumer application stream.
    pub fn register_consumer(&mut self, consumer_id: &str) -> &ConsumerStream {
        self.consumers
            .entry(consumer_id.to_string())
            .or_insert_with(|| ConsumerStream {
                id: consumer_id.to_string(),
                bound_node_id: None,
                needs_rebind: true,
            })
    }

    /// Checks if a consumer stream needs to rebind to the active virtual source node.
    #[must_use]
    pub fn consumer_needs_rebind(&self, consumer_id: &str) -> bool {
        self.consumers.get(consumer_id).is_none_or(|consumer| {
            consumer.needs_rebind || consumer.bound_node_id != Some(self.current_node_id)
        })
    }

    /// Rebinds a consumer stream to the current active virtual source node.
    pub fn rebind_consumer(&mut self, consumer_id: &str) -> Result<(), EndpointError> {
        if self.status == EndpointStatus::Closed {
            return Err(EndpointError::IoError("Endpoint closed".to_string()));
        }

        let current_node = self.current_node_id;
        let consumer = self
            .consumers
            .entry(consumer_id.to_string())
            .or_insert_with(|| ConsumerStream {
                id: consumer_id.to_string(),
                bound_node_id: None,
                needs_rebind: true,
            });

        consumer.bound_node_id = Some(current_node);
        consumer.needs_rebind = false;
        Ok(())
    }

    /// Simulates recreation of the `PipeWire` virtual source node (e.g. after helper process restart).
    ///
    /// Assigns a new node ID, marks all existing consumer streams as requiring rebind,
    /// and resets the audio engine generation to `Warming` state.
    pub fn recreate_source(&mut self) -> u32 {
        self.current_node_id = self.current_node_id.saturating_add(1);

        for consumer in self.consumers.values_mut() {
            consumer.needs_rebind = true;
            consumer.bound_node_id = None;
        }

        self.active_generation = self.active_generation.next();
        self.generation_state = GenerationState::Warming;
        self.current_node_id
    }

    /// Reads an audio frame for a designated consumer stream.
    ///
    /// ENFORCES FAIL-CLOSED DIGITAL SILENCE POLICY:
    /// Returns pure zeros (`[0.0; HOP_SAMPLES]`) whenever:
    /// - Endpoint is not Active
    /// - Supervisor state is `EngineUnavailable`, `Restarting`, or `TerminalSafeState`
    /// - Operating mode is `Mute`
    /// - Consumer requires rebind or is bound to an obsolete node ID
    /// - Audio generation is in `Warming` or `Closed` state
    #[must_use]
    pub fn read_frame_for_consumer(&self, consumer_id: &str) -> AudioFrame {
        const SILENCE: AudioFrame = [0.0; HOP_SAMPLES];

        // 1. Endpoint must be active
        if self.status != EndpointStatus::Active {
            return SILENCE;
        }

        // 2. Supervisor state check (fail-closed digital silence)
        if self.supervisor_state != SupervisorState::Running {
            return SILENCE;
        }

        // 3. Operating mode check
        if self.denoise_mode == DenoiseMode::Mute {
            return SILENCE;
        }

        // 4. Consumer existence check
        let Some(consumer) = self.consumers.get(consumer_id) else {
            return SILENCE;
        };

        // 5. Consumer node binding check
        if consumer.needs_rebind || consumer.bound_node_id != Some(self.current_node_id) {
            return SILENCE;
        }

        // 6. Generation warming check
        if self.generation_state != GenerationState::Active {
            return SILENCE;
        }

        // 7. Deliver validated audio frame
        self.latest_frame
    }
}

impl VirtualMicrophone for LinuxVirtualMicrophone {
    fn start(&mut self) -> Result<(), EndpointError> {
        if self.is_contended {
            self.status = EndpointStatus::UnavailableBusy;
            return Err(EndpointError::DeviceBusy);
        }

        self.status = EndpointStatus::Active;
        Ok(())
    }

    fn stop(&mut self) -> Result<(), EndpointError> {
        self.status = EndpointStatus::Closed;
        Ok(())
    }

    fn status(&self) -> EndpointStatus {
        self.status
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(clippy::float_cmp)]
    fn test_silence_on_supervisor_states() {
        let mut mic = LinuxVirtualMicrophone::new();
        assert!(mic.start().is_ok());

        let consumer_id = "test-consumer";
        mic.register_consumer(consumer_id);
        assert!(mic.rebind_consumer(consumer_id).is_ok());
        mic.set_generation(GenerationId::new(1), GenerationState::Active);

        let test_frame: AudioFrame = [0.75; HOP_SAMPLES];
        mic.push_frame(test_frame);

        // Active running: receives audio
        assert_eq!(mic.read_frame_for_consumer(consumer_id), test_frame);

        // 1. EngineUnavailable -> Silence
        mic.set_supervisor_state(SupervisorState::EngineUnavailable);
        assert_eq!(mic.read_frame_for_consumer(consumer_id), [0.0; HOP_SAMPLES]);

        // 2. Restarting -> Silence
        mic.set_supervisor_state(SupervisorState::Restarting);
        assert_eq!(mic.read_frame_for_consumer(consumer_id), [0.0; HOP_SAMPLES]);

        // 3. TerminalSafeState -> Silence
        mic.set_supervisor_state(SupervisorState::TerminalSafeState);
        assert_eq!(mic.read_frame_for_consumer(consumer_id), [0.0; HOP_SAMPLES]);

        // Recover to Running -> Active audio resumed
        mic.set_supervisor_state(SupervisorState::Running);
        assert!(mic.start().is_ok());
        assert_eq!(mic.read_frame_for_consumer(consumer_id), test_frame);

        // 4. Mute mode -> Silence
        mic.set_denoise_mode(DenoiseMode::Mute);
        assert_eq!(mic.read_frame_for_consumer(consumer_id), [0.0; HOP_SAMPLES]);
    }
}
