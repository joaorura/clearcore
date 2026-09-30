#![forbid(unsafe_code)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::float_cmp,
    clippy::cast_precision_loss
)]

use realtime_noise_contracts::{AudioFrame, HOP_SAMPLES};
use realtime_noise_format_adapter::{InputAccumulator, OutputDeframer};

#[test]
fn accumulator_emits_one_480_sample_frame_from_irregular_callbacks() {
    let mut accumulator = InputAccumulator::new();
    assert!(accumulator.is_empty());
    assert_eq!(accumulator.available_samples(), 0);

    // Feed irregular callback 1: 127 samples
    let chunk1: Vec<f32> = (0..127).map(|i| i as f32).collect();
    accumulator.push_samples(&chunk1);

    // Must not produce a frame yet
    assert_eq!(accumulator.pop_frame(), None);
    assert_eq!(accumulator.available_samples(), 127);
    assert!(!accumulator.is_empty());

    // Feed irregular callback 2: 353 samples (127 + 353 = 480)
    let chunk2: Vec<f32> = (127..480).map(|i| i as f32).collect();
    accumulator.push_samples(&chunk2);

    // Must produce exactly one 480-sample frame
    let frame = accumulator
        .pop_frame()
        .expect("exactly one 480-sample frame must be produced");

    assert_eq!(accumulator.pop_frame(), None);
    assert_eq!(accumulator.available_samples(), 0);
    assert!(accumulator.is_empty());

    // Assert exact sequence of values
    assert_eq!(frame.len(), HOP_SAMPLES);
    for (i, &sample) in frame.iter().enumerate() {
        assert_eq!(sample, i as f32, "mismatch at sample index {i}");
    }
}

#[test]
fn deframer_variable_chunk_output_and_underrun_silence() {
    let mut deframer = OutputDeframer::new();
    assert!(deframer.is_empty());
    assert_eq!(deframer.available_samples(), 0);
    assert_eq!(deframer.underrun_count(), 0);

    // Push one 480-sample frame
    let mut frame: AudioFrame = [0.0; HOP_SAMPLES];
    for (i, sample) in frame.iter_mut().enumerate() {
        *sample = (i as f32) + 1.0;
    }
    deframer
        .push_frame(&frame)
        .expect("pushing frame must succeed");

    assert_eq!(deframer.available_samples(), 480);
    assert!(!deframer.is_empty());

    // Pull irregular chunk 1: 127 samples
    let mut out_chunk1 = [0.0f32; 127];
    deframer.fill_slice(&mut out_chunk1);
    assert_eq!(deframer.available_samples(), 353);
    for (i, &sample) in out_chunk1.iter().enumerate() {
        assert_eq!(sample, (i as f32) + 1.0);
    }
    assert_eq!(deframer.underrun_count(), 0);

    // Pull irregular chunk 2: 353 samples
    let mut out_chunk2 = [0.0f32; 353];
    deframer.fill_slice(&mut out_chunk2);
    assert_eq!(deframer.available_samples(), 0);
    assert!(deframer.is_empty());
    for (i, &sample) in out_chunk2.iter().enumerate() {
        assert_eq!(sample, ((127 + i) as f32) + 1.0);
    }
    assert_eq!(deframer.underrun_count(), 0);

    // Pull on underrun (buffer empty) -> must fill with silence (0.0) and record underrun
    let mut underrun_chunk = [99.0f32; 64];
    deframer.fill_slice(&mut underrun_chunk);
    assert_eq!(deframer.underrun_count(), 1);
    for &sample in &underrun_chunk {
        assert_eq!(sample, 0.0);
    }
}

#[test]
fn deframer_partial_underrun_fills_remainder_with_silence() {
    let mut deframer = OutputDeframer::new();
    let frame: AudioFrame = [42.0; HOP_SAMPLES];
    deframer.push_frame(&frame).unwrap();

    // Drain 460 samples leaving 20 samples in deframer
    let mut sink = [0.0f32; 460];
    deframer.fill_slice(&mut sink);
    assert_eq!(deframer.available_samples(), 20);

    // Request 50 samples: should get 20 of 42.0 and 30 of 0.0
    let mut partial = [1.0f32; 50];
    deframer.fill_slice(&mut partial);
    assert_eq!(deframer.underrun_count(), 1);
    for &s in &partial[..20] {
        assert_eq!(s, 42.0);
    }
    for &s in &partial[20..] {
        assert_eq!(s, 0.0);
    }
}
