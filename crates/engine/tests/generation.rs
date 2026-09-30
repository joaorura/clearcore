#![forbid(unsafe_code)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::float_cmp)]

use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use realtime_noise_contracts::{
    AudioFrame, Discontinuity, FrameEnvelope, HOP_SAMPLES, RealtimeTransport, TransportFull,
};
use realtime_noise_engine::{DenoiseEngine, DenoiseMode};
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

struct SpyBackend {
    descriptor: BackendDescriptor,
    process_calls: Arc<AtomicUsize>,
}

impl SpyBackend {
    fn new(process_calls: Arc<AtomicUsize>) -> Self {
        Self {
            descriptor: BackendDescriptor {
                backend: "mock-spy",
                backend_version: "0.1.0",
                runtime: "mock",
                runtime_version: "0.1.0",
                asset_id: "mock-asset".to_owned(),
                asset_sha256: "0".repeat(64),
                cpu_profile: "mock",
            },
            process_calls,
        }
    }
}

impl InferenceBackend for SpyBackend {
    fn descriptor(&self) -> BackendDescriptor {
        self.descriptor.clone()
    }

    fn process(&mut self, input: &AudioFrame) -> Result<ProcessedFrame, InferenceError> {
        self.process_calls.fetch_add(1, Ordering::SeqCst);
        ProcessedFrame::checked(*input, 0, self.descriptor.clone())
    }

    fn algorithmic_latency_samples(&self) -> u32 {
        0
    }
}

#[test]
fn bypass_keeps_framing_without_calling_inference_or_raw_fallback() {
    let input = Arc::new(TestTransport::new(24));
    let output = Arc::new(TestTransport::new(24));
    let process_calls = Arc::new(AtomicUsize::new(0));
    let backend = Box::new(SpyBackend::new(process_calls.clone()));
    let mut engine = DenoiseEngine::new(input.clone(), output.clone(), backend);

    engine.set_mode(DenoiseMode::Bypass);
    engine
        .start()
        .expect("engine must start successfully in bypass mode");

    let mut test_samples = [0.0; HOP_SAMPLES];
    for (i, sample) in test_samples.iter_mut().enumerate() {
        #[allow(clippy::cast_precision_loss)]
        let val = (i as f32) / (HOP_SAMPLES as f32);
        *sample = val;
    }

    let input_frame = FrameEnvelope {
        samples: test_samples,
        sequence: 42,
        capture_monotonic_ns: 420_000_000,
        generation: 1,
        discontinuity: Discontinuity::NONE,
    };
    input
        .try_push(input_frame)
        .expect("pushing frame to input transport must succeed");

    // Allow worker time to process
    std::thread::sleep(Duration::from_millis(50));

    let processed = output
        .try_pop()
        .expect("frame must be delivered to output in bypass mode");

    assert_eq!(
        process_calls.load(Ordering::SeqCst),
        0,
        "inference backend process must NOT be called when in bypass mode"
    );
    assert_eq!(
        processed.samples, test_samples,
        "framing and samples must be preserved in bypass mode"
    );
    assert_eq!(processed.sequence, 42, "sequence number must be preserved");
    assert_eq!(
        processed.capture_monotonic_ns, 420_000_000,
        "capture timestamp must be preserved"
    );

    engine.stop().expect("engine must stop cleanly");
}
