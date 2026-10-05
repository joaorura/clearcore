#![forbid(unsafe_code)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::float_cmp)]

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use realtime_noise_contracts::{
    AudioFrame, Discontinuity, FrameEnvelope, HOP_SAMPLES, RealtimeTransport, TransportFull,
};
use realtime_noise_engine::DenoiseEngine;
use realtime_noise_model::{BackendDescriptor, InferenceBackend, InferenceError, ProcessedFrame};

struct TestTransport {
    capacity: usize,
    queue: Mutex<VecDeque<FrameEnvelope>>,
}

impl TestTransport {
    fn new(capacity: usize) -> Self {
        Self {
            capacity,
            queue: Mutex::new(VecDeque::with_capacity(capacity)),
        }
    }

    fn queue(&self) -> MutexGuard<'_, VecDeque<FrameEnvelope>> {
        self.queue.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl RealtimeTransport for TestTransport {
    fn try_push(&self, frame: FrameEnvelope) -> Result<(), TransportFull> {
        let mut queue = self.queue();
        if queue.len() >= self.capacity {
            return Err(TransportFull);
        }
        queue.push_back(frame);
        drop(queue);
        Ok(())
    }

    fn try_pop(&self) -> Option<FrameEnvelope> {
        self.queue().pop_front()
    }

    fn capacity_hops(&self) -> usize {
        self.capacity
    }

    fn backlog_hops(&self) -> usize {
        self.queue().len()
    }

    fn close_generation(&self, _generation: u64) {}
}

struct SlowBackend {
    descriptor: BackendDescriptor,
    delay: Duration,
}

impl SlowBackend {
    fn new(delay: Duration) -> Self {
        Self {
            descriptor: BackendDescriptor {
                backend: "mock-slow",
                backend_version: "0.1.0",
                runtime: "mock",
                runtime_version: "0.1.0",
                asset_id: "mock-asset".to_owned(),
                asset_sha256: "0".repeat(64),
                cpu_profile: "mock",
            },
            delay,
        }
    }
}

impl InferenceBackend for SlowBackend {
    fn descriptor(&self) -> BackendDescriptor {
        self.descriptor.clone()
    }

    fn process(&mut self, input: &AudioFrame) -> Result<ProcessedFrame, InferenceError> {
        std::thread::sleep(self.delay);
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
fn inference_deadline_miss_closes_generation_and_outputs_silence() {
    let input = Arc::new(TestTransport::new(24));
    let output = Arc::new(TestTransport::new(24));
    // Hard deadline is 10.0 ms. Set backend delay to 15 ms to trigger deadline miss.
    let backend = Box::new(SlowBackend::new(Duration::from_millis(15)));
    let mut engine = DenoiseEngine::new(input.clone(), output.clone(), backend);

    engine
        .start()
        .expect("engine must start successfully before processing frames");

    let input_frame = FrameEnvelope {
        samples: [0.75; HOP_SAMPLES],
        sequence: 1,
        capture_monotonic_ns: 10_000_000,
        generation: 1,
        discontinuity: Discontinuity::NONE,
    };
    input
        .try_push(input_frame)
        .expect("pushing frame to input transport must succeed");

    // Allow worker time to process and encounter the deadline miss
    std::thread::sleep(Duration::from_millis(50));

    let processed = output
        .try_pop()
        .expect("engine must output a frame even upon deadline miss");

    assert_eq!(
        processed.samples, [0.0; HOP_SAMPLES],
        "deadline miss must emit digital silence (fail-closed)"
    );
    assert!(
        processed
            .discontinuity
            .contains(Discontinuity::INFERENCE_DEADLINE_MISS),
        "discontinuity must include INFERENCE_DEADLINE_MISS"
    );
    assert_eq!(
        engine.status().deadline_miss_count(),
        1,
        "engine status must reflect the deadline miss"
    );

    engine.stop().expect("engine must stop cleanly");
}
