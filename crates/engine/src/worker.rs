#![forbid(unsafe_code)]
#![allow(
    clippy::missing_errors_doc,
    clippy::missing_const_for_fn,
    clippy::significant_drop_tightening,
    clippy::too_many_lines
)]

use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use realtime_noise_contracts::{
    AudioFrame, Discontinuity, FrameEnvelope, HOP_SAMPLES, RealtimeTransport,
};
use realtime_noise_model::InferenceError;

use crate::engine::{
    DenoiseMode, EngineSharedState, INFERENCE_HARD_DEADLINE, ResetReason, apply_profile_update,
};
use crate::queue::DEFAULT_WATERMARK_HOPS;

/// Real-time noise worker processing engine.
pub struct DenoiseWorker;

impl DenoiseWorker {
    /// Worker thread main processing loop running off the audio callback thread.
    pub(crate) fn run_loop(
        shared: &Arc<Mutex<EngineSharedState>>,
        input: &Arc<dyn RealtimeTransport>,
        output: &Arc<dyn RealtimeTransport>,
    ) {
        while {
            let state = shared.lock().unwrap_or_else(PoisonError::into_inner);
            state.is_running
        } {
            let backlog = input.backlog_hops();
            if let Some(frame) = input.try_pop() {
                Self::process_frame(shared, input, output, &frame, backlog);
            } else {
                thread::sleep(Duration::from_micros(200));
            }
        }
    }

    /// Processes a single audio frame and delivers to output transport.
    pub(crate) fn process_frame(
        shared: &Arc<Mutex<EngineSharedState>>,
        input: &Arc<dyn RealtimeTransport>,
        output: &Arc<dyn RealtimeTransport>,
        input_frame: &FrameEnvelope,
        input_backlog: usize,
    ) {
        let mut state = shared.lock().unwrap_or_else(PoisonError::into_inner);

        // Apply any pending backend switch at 480-sample hop boundary
        if let Some(pending) = state.pending_backend.take() {
            state.backend = Some(pending);
            // The new backend starts neutral; a pending update below restores the right id.
            state.applied_voice_profile_id = None;
            state.voice_profile_error = None;
        }

        // Apply any pending voice profile update here, before the timed inference region, so a
        // blocking backend round trip never counts toward the 10 ms deadline.
        if let Some(update) = state.pending_voice_profile.take() {
            apply_profile_update(&mut state, &update);
        }

        // Check if queue age watermark is exceeded (> 2 hops / 20 ms)
        if input_backlog > DEFAULT_WATERMARK_HOPS {
            let (old_id, next_id) =
                state.advance_generation(ResetReason::QueueAgeWatermarkExceeded);
            drop(state);

            // Flush stale items belonging to superseded generation from consumer queues
            input.close_generation(old_id);
            output.close_generation(old_id);

            // Emit fail-closed digital silence with GENERATION_CHANGE discontinuity
            let envelope = FrameEnvelope {
                samples: [0.0; HOP_SAMPLES],
                sequence: input_frame.sequence,
                capture_monotonic_ns: input_frame.capture_monotonic_ns,
                generation: next_id.get(),
                discontinuity: input_frame.discontinuity | Discontinuity::GENERATION_CHANGE,
            };
            let _ = output.try_push(envelope);
            return;
        }

        let mode = state.mode;
        let output_samples: AudioFrame;
        let output_discontinuity: Discontinuity;
        let output_generation: u64;

        match mode {
            DenoiseMode::Mute => {
                output_samples = [0.0; HOP_SAMPLES];
                output_discontinuity = input_frame.discontinuity;
                output_generation = state.generation.id().get();
            }
            DenoiseMode::Bypass => {
                output_samples = input_frame.samples;
                output_discontinuity = input_frame.discontinuity;
                output_generation = state.generation.id().get();
            }
            DenoiseMode::Active => {
                let start = Instant::now();
                let process_result = state.backend.as_mut().map_or_else(
                    || {
                        Err(InferenceError::UnsupportedCpuProfile(
                            "no backend available".to_owned(),
                        ))
                    },
                    |backend| backend.process(&input_frame.samples),
                );
                let elapsed = start.elapsed();

                if elapsed > INFERENCE_HARD_DEADLINE {
                    // Hard deadline miss (> 10.0 ms): fail-closed silence policy
                    state.deadline_miss_count = state.deadline_miss_count.saturating_add(1);
                    let (old_id, next_id) =
                        state.advance_generation(ResetReason::InferenceDeadlineMiss);
                    drop(state);

                    output.close_generation(old_id);

                    let envelope = FrameEnvelope {
                        samples: [0.0; HOP_SAMPLES],
                        sequence: input_frame.sequence,
                        capture_monotonic_ns: input_frame.capture_monotonic_ns,
                        generation: next_id.get(),
                        discontinuity: input_frame.discontinuity
                            | Discontinuity::INFERENCE_DEADLINE_MISS
                            | Discontinuity::GENERATION_CHANGE,
                    };
                    let _ = output.try_push(envelope);
                    return;
                }

                let Ok(processed_frame) = process_result else {
                    // Backend processing error: fail-closed silence policy
                    state.deadline_miss_count = state.deadline_miss_count.saturating_add(1);
                    let (old_id, next_id) =
                        state.advance_generation(ResetReason::InferenceDeadlineMiss);
                    drop(state);

                    output.close_generation(old_id);

                    let envelope = FrameEnvelope {
                        samples: [0.0; HOP_SAMPLES],
                        sequence: input_frame.sequence,
                        capture_monotonic_ns: input_frame.capture_monotonic_ns,
                        generation: next_id.get(),
                        discontinuity: input_frame.discontinuity
                            | Discontinuity::INFERENCE_DEADLINE_MISS
                            | Discontinuity::GENERATION_CHANGE,
                    };
                    let _ = output.try_push(envelope);
                    return;
                };

                output_samples = processed_frame.samples;
                output_discontinuity = input_frame.discontinuity;
                output_generation = state.generation.id().get();
            }
        }

        drop(state);

        let envelope = FrameEnvelope {
            samples: output_samples,
            sequence: input_frame.sequence,
            capture_monotonic_ns: input_frame.capture_monotonic_ns,
            generation: output_generation,
            discontinuity: output_discontinuity,
        };
        let _ = output.try_push(envelope);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{EngineState, StudioAttachment};
    use crate::generation::{Generation, GenerationId};
    use crate::queue::BoundedQueueTransport;
    use realtime_noise_model::{
        BackendDescriptor, InferenceBackend, ProcessedFrame, StudioResetHandle,
    };
    use studio_dsp::{Preset, StudioControl};

    /// Echoes the input, or fails every call when `fail` is set.
    struct ScriptedBackend {
        fail: bool,
    }

    impl ScriptedBackend {
        fn descriptor() -> BackendDescriptor {
            BackendDescriptor {
                backend: "scripted",
                backend_version: "1",
                runtime: "test",
                runtime_version: "1",
                asset_id: "test".to_owned(),
                asset_sha256: "0".repeat(64),
                cpu_profile: "test",
            }
        }
    }

    impl InferenceBackend for ScriptedBackend {
        fn descriptor(&self) -> BackendDescriptor {
            Self::descriptor()
        }

        fn process(&mut self, input: &AudioFrame) -> Result<ProcessedFrame, InferenceError> {
            if self.fail {
                return Err(InferenceError::InferenceExecution("scripted".to_owned()));
            }
            ProcessedFrame::checked(*input, 0, Self::descriptor())
        }

        fn algorithmic_latency_samples(&self) -> u32 {
            0
        }

        fn set_voice_profile(
            &mut self,
            profile: Option<&realtime_noise_model::VoiceProfile>,
        ) -> Result<(), realtime_noise_model::InferenceError> {
            realtime_noise_model::reject_unsupported_voice_profile(profile)
        }
    }

    fn shared_state(fail: bool, reset: &StudioResetHandle) -> Arc<Mutex<EngineSharedState>> {
        let control = Arc::new(StudioControl::new(Preset::Off));
        Arc::new(Mutex::new(EngineSharedState {
            state: EngineState::Running,
            mode: DenoiseMode::Active,
            backend: Some(Box::new(ScriptedBackend { fail })),
            pending_backend: None,
            generation: Generation::active(GenerationId::new(1)),
            deadline_miss_count: 0,
            is_running: true,
            studio: Some(StudioAttachment::new(control, reset.clone())),
            pending_voice_profile: None,
            applied_voice_profile_id: None,
            voice_profile_error: None,
        }))
    }

    fn input_frame() -> FrameEnvelope {
        FrameEnvelope {
            samples: [0.25; HOP_SAMPLES],
            sequence: 7,
            capture_monotonic_ns: 1,
            generation: 1,
            discontinuity: Discontinuity::NONE,
        }
    }

    fn run_one_frame(shared: &Arc<Mutex<EngineSharedState>>, backlog: usize) {
        let input: Arc<dyn RealtimeTransport> = Arc::new(BoundedQueueTransport::new());
        let output: Arc<dyn RealtimeTransport> = Arc::new(BoundedQueueTransport::new());
        DenoiseWorker::process_frame(shared, &input, &output, &input_frame(), backlog);
    }

    fn generation_id(shared: &Arc<Mutex<EngineSharedState>>) -> u64 {
        let state = shared.lock().unwrap_or_else(PoisonError::into_inner);
        state.generation.id().get()
    }

    #[test]
    fn healthy_frame_does_not_request_a_studio_reset() {
        let reset = StudioResetHandle::new();
        let shared = shared_state(false, &reset);
        run_one_frame(&shared, 0);
        assert!(!reset.is_requested());
        assert_eq!(generation_id(&shared), 1);
    }

    #[test]
    fn queue_watermark_closure_requests_a_studio_reset() {
        let reset = StudioResetHandle::new();
        let shared = shared_state(false, &reset);
        run_one_frame(&shared, DEFAULT_WATERMARK_HOPS + 1);
        assert!(reset.is_requested());
        assert_eq!(generation_id(&shared), 2);
    }

    #[test]
    fn backend_error_closure_requests_a_studio_reset() {
        let reset = StudioResetHandle::new();
        let shared = shared_state(true, &reset);
        run_one_frame(&shared, 0);
        assert!(reset.is_requested());
        assert_eq!(generation_id(&shared), 2);
    }
}
