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

use crate::engine::{DenoiseMode, EngineSharedState, INFERENCE_HARD_DEADLINE, ResetReason};
use crate::generation::Generation;
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
        }

        // Check if queue age watermark is exceeded (> 2 hops / 20 ms)
        if input_backlog > DEFAULT_WATERMARK_HOPS {
            let old_id = state.generation.id().get();
            state
                .generation
                .close(ResetReason::QueueAgeWatermarkExceeded);
            let next_id = state.generation.id().next();
            state.generation = Generation::active(next_id);
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
                    let old_id = state.generation.id().get();
                    state.generation.close(ResetReason::InferenceDeadlineMiss);
                    let next_id = state.generation.id().next();
                    state.generation = Generation::active(next_id);
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
                    let old_id = state.generation.id().get();
                    state.generation.close(ResetReason::InferenceDeadlineMiss);
                    let next_id = state.generation.id().next();
                    state.generation = Generation::active(next_id);
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
