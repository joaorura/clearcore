#![forbid(unsafe_code)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::float_cmp)]

use realtime_noise_contracts::HOP_SAMPLES;
use realtime_noise_format_adapter::InputAccumulator;

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
