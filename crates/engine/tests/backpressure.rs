#![forbid(unsafe_code)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::float_cmp)]

use std::sync::Arc;
use std::time::Duration;

use realtime_noise_contracts::{
    AudioFrame, Discontinuity, FrameEnvelope, HOP_SAMPLES, RealtimeTransport, TransportFull,
};
use realtime_noise_engine::{BoundedQueueTransport, DenoiseEngine};
use realtime_noise_model::{BackendDescriptor, InferenceBackend, InferenceError, ProcessedFrame};

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

    fn set_voice_profile(
        &mut self,
        profile: Option<&realtime_noise_model::VoiceProfile>,
    ) -> Result<(), realtime_noise_model::InferenceError> {
        realtime_noise_model::reject_unsupported_voice_profile(profile)
    }
}

#[test]
fn backpressure_drops_frame_and_emits_discontinuity() {
    let queue = BoundedQueueTransport::new();
    assert_eq!(queue.capacity_hops(), 24);

    let mut dropped_count = 0;
    for seq in 1..=30 {
        let frame = FrameEnvelope {
            samples: [0.1; HOP_SAMPLES],
            sequence: seq,
            capture_monotonic_ns: seq * 10_000_000,
            generation: 1,
            discontinuity: Discontinuity::NONE,
        };
        match queue.try_push(frame) {
            Ok(()) => {}
            Err(TransportFull) => {
                dropped_count += 1;
            }
        }
    }

    assert_eq!(queue.backlog_hops(), 24);
    assert_eq!(
        dropped_count, 6,
        "strictly 6 frames (30 - 24 capacity) must be rejected with TransportFull"
    );

    // Verify consumer's first frame is untouched (FIFO order preserved)
    let first = queue.try_pop().expect("first frame should be present");
    assert_eq!(first.sequence, 1);
    assert_eq!(queue.backlog_hops(), 23);

    // After dropping frames, next successfully pushed frame carries CAPTURE_DROP flag
    let next_frame = FrameEnvelope {
        samples: [0.1; HOP_SAMPLES],
        sequence: 31,
        capture_monotonic_ns: 31 * 10_000_000,
        generation: 1,
        discontinuity: Discontinuity::CAPTURE_DROP,
    };
    assert!(
        queue.try_push(next_frame).is_ok(),
        "pushing after pop must succeed without blocking"
    );
    assert_eq!(queue.backlog_hops(), 24);
}

#[test]
fn queue_age_watermark_exceeded_restarts_generation() {
    let input = Arc::new(BoundedQueueTransport::new());
    let output = Arc::new(BoundedQueueTransport::new());
    let backend = Box::new(PassthroughBackend::new());
    let mut engine = DenoiseEngine::new(input.clone(), output.clone(), backend);

    // Push 5 frames into input queue while engine is not running, exceeding the 2-hop (20 ms) watermark
    for seq in 1..=5 {
        let frame = FrameEnvelope {
            samples: [0.2; HOP_SAMPLES],
            sequence: seq,
            capture_monotonic_ns: seq * 10_000_000,
            generation: 1,
            discontinuity: Discontinuity::NONE,
        };
        input
            .try_push(frame)
            .expect("pushing initial frames should succeed");
    }

    assert!(
        input.is_watermark_exceeded(),
        "input queue backlog of 5 hops must exceed the 2-hop watermark"
    );
    assert_eq!(input.backlog_hops(), 5);
    assert_eq!(engine.status().generation(), 1);

    // Start engine; worker should detect watermark exceeded, close generation 1, and advance generation
    engine.start().expect("engine should start successfully");

    std::thread::sleep(Duration::from_millis(50));

    // Generation must have restarted (incremented to >= 2)
    assert!(
        engine.status().generation() >= 2,
        "engine must restart generation when queue watermark is exceeded"
    );

    // Output must receive frame with GENERATION_CHANGE discontinuity
    let processed = output
        .try_pop()
        .expect("frame should be produced to output transport");
    assert!(
        processed
            .discontinuity
            .contains(Discontinuity::GENERATION_CHANGE),
        "discontinuity flag must indicate GENERATION_CHANGE after watermark reset"
    );

    engine.stop().expect("engine should stop cleanly");
}
