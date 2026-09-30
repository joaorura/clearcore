#![forbid(unsafe_code)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::float_cmp,
    clippy::cast_precision_loss,
    clippy::significant_drop_tightening
)]

use std::collections::VecDeque;
use std::sync::Mutex;

use realtime_noise_contracts::{
    Discontinuity, FrameEnvelope, HOP_SAMPLES, RealtimeTransport, TransportFull,
};
use realtime_noise_format_adapter::{
    AudioEndpointConfig, FormatAdapterWorker, FormatError, IdentityResampler, LinearResampler,
    Resampler, SampleFormat,
};

#[derive(Default)]
struct TestTransport {
    capacity: usize,
    queue: Mutex<VecDeque<FrameEnvelope>>,
}

impl TestTransport {
    fn with_capacity(capacity: usize) -> Self {
        Self {
            capacity,
            queue: Mutex::new(VecDeque::with_capacity(capacity)),
        }
    }
}

impl RealtimeTransport for TestTransport {
    fn try_push(&self, frame: FrameEnvelope) -> Result<(), TransportFull> {
        let mut q = self.queue.lock().unwrap();
        if q.len() >= self.capacity {
            return Err(TransportFull);
        }
        q.push_back(frame);
        drop(q);
        Ok(())
    }

    fn try_pop(&self) -> Option<FrameEnvelope> {
        self.queue.lock().unwrap().pop_front()
    }

    fn capacity_hops(&self) -> usize {
        self.capacity
    }

    fn backlog_hops(&self) -> usize {
        self.queue.lock().unwrap().len()
    }

    fn close_generation(&self, generation: u64) {
        self.queue
            .lock()
            .unwrap()
            .retain(|f| f.generation != generation);
    }
}

#[test]
fn non_48khz_inputs_fail_closed_without_explicit_resampler() {
    let input_transport = TestTransport::with_capacity(10);
    let output_transport = TestTransport::with_capacity(10);

    // 44.1 kHz input without explicit resampler must be rejected
    let config_44k = AudioEndpointConfig::new(44_100, 1, SampleFormat::Float32);
    let config_48k = AudioEndpointConfig::new(48_000, 1, SampleFormat::Float32);

    let worker_res =
        FormatAdapterWorker::new(input_transport, output_transport, config_44k, config_48k);
    assert_eq!(
        worker_res.err(),
        Some(FormatError::UnsupportedSampleRate(44_100))
    );

    // IdentityResampler directly rejects non-48kHz
    assert_eq!(
        IdentityResampler::new(44_100).err(),
        Some(FormatError::UnsupportedSampleRate(44_100))
    );
    assert_eq!(
        IdentityResampler::new(16_000).err(),
        Some(FormatError::UnsupportedSampleRate(16_000))
    );

    // 48 kHz IdentityResampler succeeds
    let identity = IdentityResampler::new(48_000).expect("48 kHz identity resampler must succeed");
    assert_eq!(identity.source_rate_hz(), 48_000);
    assert_eq!(identity.target_rate_hz(), 48_000);
}

#[test]
fn explicit_resampler_converts_rates_and_coordinates_worker() {
    let input_transport = TestTransport::with_capacity(10);
    let output_transport = TestTransport::with_capacity(10);

    let config_16k = AudioEndpointConfig::new(16_000, 1, SampleFormat::Float32);
    let config_48k = AudioEndpointConfig::new(48_000, 1, SampleFormat::Float32);

    // 16 kHz to 48 kHz (3x upsampling) with explicit resampler
    let resampler = Box::new(LinearResampler::new(16_000, 48_000).unwrap());

    let mut worker = FormatAdapterWorker::with_resamplers(
        input_transport,
        output_transport,
        config_16k,
        config_48k,
        Some(resampler),
        None,
    )
    .expect("worker with explicit resampler must construct successfully");

    // Feed 160 samples at 16 kHz (10 ms of audio -> 480 samples @ 48 kHz)
    let input_16k = vec![0.5f32; 160];
    let frames_pushed = worker
        .on_input_f32(&input_16k, 10_000_000)
        .expect("on_input_f32 should succeed");

    assert_eq!(
        frames_pushed, 1,
        "160 samples @ 16 kHz must resample to exactly one 480-sample frame @ 48 kHz"
    );
}

#[test]
fn worker_handles_stereo_pcm16_irregular_chunks_and_backpressure() {
    let input_transport = TestTransport::with_capacity(2);
    let output_transport = TestTransport::with_capacity(2);

    let in_config = AudioEndpointConfig::new(48_000, 2, SampleFormat::Pcm16);
    let out_config = AudioEndpointConfig::new(48_000, 2, SampleFormat::Float32);

    let mut worker =
        FormatAdapterWorker::new(input_transport, output_transport, in_config, out_config)
            .expect("worker creation must succeed");

    // Feed irregular stereo chunk 1: 127 stereo samples (254 i16 values)
    // Left: 16384 (approx 0.5), Right: 16384 -> mono approx 0.5
    let chunk1 = vec![16384i16; 254];
    let frames1 = worker
        .on_input_pcm16(&chunk1, 1_000_000)
        .expect("chunk1 push must succeed");
    assert_eq!(frames1, 0, "no frame yet after 127 samples");

    // Feed irregular stereo chunk 2: 353 stereo samples (706 i16 values)
    let chunk2 = vec![16384i16; 706];
    let frames2 = worker
        .on_input_pcm16(&chunk2, 2_000_000)
        .expect("chunk2 push must succeed");
    assert_eq!(
        frames2, 1,
        "exactly 1 frame emitted after 127 + 353 = 480 samples"
    );
}

#[test]
fn worker_detects_drop_and_sets_capture_drop_discontinuity() {
    let input_transport = TestTransport::with_capacity(1);
    let output_transport = TestTransport::with_capacity(1);

    let config = AudioEndpointConfig::new(48_000, 1, SampleFormat::Float32);
    let mut worker =
        FormatAdapterWorker::new(input_transport, output_transport, config, config).unwrap();

    // Push 480 samples -> fills queue to capacity (1 frame)
    let frame1 = [0.1f32; HOP_SAMPLES];
    let p1 = worker.on_input_f32(&frame1, 1_000_000).unwrap();
    assert_eq!(p1, 1);

    // Push second 480 samples -> transport is full (capacity=1), drop occurs!
    let frame2 = [0.2f32; HOP_SAMPLES];
    let p2 = worker.on_input_f32(&frame2, 2_000_000).unwrap();
    assert_eq!(p2, 0, "frame should be dropped due to backpressure");

    // Pop the first frame from transport to free space
    let popped1 = worker
        .input_transport
        .try_pop()
        .expect("first frame should be present");
    assert_eq!(popped1.discontinuity, Discontinuity::NONE);

    // Push third 480 samples -> should succeed and carry CAPTURE_DROP flag!
    let frame3 = [0.3f32; HOP_SAMPLES];
    let p3 = worker.on_input_f32(&frame3, 3_000_000).unwrap();
    assert_eq!(p3, 1, "frame must be pushed after space is freed");

    let popped3 = worker
        .input_transport
        .try_pop()
        .expect("third frame must be in queue");
    assert!(
        popped3.discontinuity.contains(Discontinuity::CAPTURE_DROP),
        "next frame after transport drop must carry CAPTURE_DROP discontinuity"
    );
}

#[test]
fn worker_output_deframer_supplies_stereo_pcm16_chunks() {
    let input_transport = TestTransport::with_capacity(5);
    let output_transport = TestTransport::with_capacity(5);

    let config_in = AudioEndpointConfig::new(48_000, 1, SampleFormat::Float32);
    let config_out = AudioEndpointConfig::new(48_000, 2, SampleFormat::Pcm16);

    let mut worker =
        FormatAdapterWorker::new(input_transport, output_transport, config_in, config_out).unwrap();

    // Push a frame into output transport
    let env = FrameEnvelope {
        samples: [0.5; HOP_SAMPLES],
        sequence: 1,
        capture_monotonic_ns: 10_000_000,
        generation: 1,
        discontinuity: Discontinuity::NONE,
    };
    worker.output_transport.try_push(env).unwrap();

    // Request 100 stereo samples (200 i16 values)
    let mut out_pcm16 = [0i16; 200];
    worker
        .on_output_pcm16(&mut out_pcm16)
        .expect("output must succeed");

    // 0.5f32 * 32767 = 16384 (within 1 LSB)
    for (idx, &val) in out_pcm16.iter().enumerate() {
        assert!(
            (val - 16384).abs() <= 1,
            "sample at {idx} should be ~16384, got {val}"
        );
    }
}
