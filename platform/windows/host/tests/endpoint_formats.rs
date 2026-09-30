#![forbid(unsafe_code)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::cast_precision_loss
)]

use realtime_noise_contracts::{EndpointError, HOP_SAMPLES, SAMPLE_RATE_HZ};
use realtime_noise_windows_host::formats::{
    audio_frame_to_f32_bytes, audio_frame_to_pcm16_bytes, canonical_f32_to_pcm16,
    f32_bytes_to_audio_frame, pcm16_bytes_to_audio_frame, pcm16_to_canonical_f32, validate_format,
    AudioEndpointFormat,
};

#[test]
fn canonical_formats_are_accepted_and_validated() {
    // Canonical Float32: 48 kHz mono 32-bit
    let fmt_f32 = validate_format(48_000, 1, 32).expect("48 kHz mono Float32 must be accepted");
    assert_eq!(fmt_f32, AudioEndpointFormat::Float32);

    // Canonical PCM16: 48 kHz mono 16-bit
    let fmt_pcm16 = validate_format(48_000, 1, 16).expect("48 kHz mono PCM16 must be accepted");
    assert_eq!(fmt_pcm16, AudioEndpointFormat::Pcm16);
}

#[test]
fn non_canonical_formats_are_rejected() {
    // Non-48kHz sample rate rejected
    let err_sr = validate_format(44_100, 1, 32).expect_err("44.1 kHz must be rejected");
    assert_eq!(
        err_sr,
        EndpointError::UnsupportedFormat(
            "unsupported sample rate 44100 Hz, canonical format requires 48000 Hz".to_string()
        )
    );

    let err_sr2 = validate_format(96_000, 1, 16).expect_err("96 kHz must be rejected");
    assert_eq!(
        err_sr2,
        EndpointError::UnsupportedFormat(
            "unsupported sample rate 96000 Hz, canonical format requires 48000 Hz".to_string()
        )
    );

    // Stereo rejected (only mono 1-channel capture supported)
    let err_ch = validate_format(SAMPLE_RATE_HZ, 2, 32).expect_err("stereo must be rejected");
    assert_eq!(
        err_ch,
        EndpointError::UnsupportedFormat(
            "unsupported channels 2, canonical format requires mono (1 channel)".to_string()
        )
    );

    // Unsupported bit depths rejected
    let err_bd = validate_format(SAMPLE_RATE_HZ, 1, 24).expect_err("24-bit must be rejected");
    assert_eq!(
        err_bd,
        EndpointError::UnsupportedFormat(
            "unsupported bit depth 24 bits, canonical format requires 16 or 32 bits".to_string()
        )
    );
}

#[test]
fn pcm16_and_float32_conversion_preserves_values_and_sanitizes() {
    let mut f32_samples = [0.0_f32; HOP_SAMPLES];
    let mut pcm16_samples = [0_i16; HOP_SAMPLES];

    // Set known values: zero, half-scale, full positive, full negative
    f32_samples[0] = 0.0;
    f32_samples[1] = 0.5;
    f32_samples[2] = 1.0;
    f32_samples[3] = -1.0;
    f32_samples[4] = 1.5; // Out of range, must clamp
    f32_samples[5] = -2.0; // Out of range, must clamp
    f32_samples[6] = f32::NAN; // Non-finite, must silence to 0
    f32_samples[7] = f32::INFINITY; // Non-finite, must silence to 0

    canonical_f32_to_pcm16(&f32_samples, &mut pcm16_samples).expect("conversion must succeed");

    assert_eq!(pcm16_samples[0], 0);
    assert!((pcm16_samples[1] - 16384).abs() <= 1);
    assert_eq!(pcm16_samples[2], 32767);
    assert_eq!(pcm16_samples[3], -32768);
    assert_eq!(pcm16_samples[4], 32767, "Over-scale must clamp to 32767");
    assert_eq!(pcm16_samples[5], -32768, "Under-scale must clamp to -32768");
    assert_eq!(pcm16_samples[6], 0, "NaN must convert to digital silence");
    assert_eq!(pcm16_samples[7], 0, "Inf must convert to digital silence");

    // Convert back from PCM16 to Float32
    let mut roundtrip_f32 = [0.0_f32; HOP_SAMPLES];
    pcm16_to_canonical_f32(&pcm16_samples, &mut roundtrip_f32).expect("conversion must succeed");

    assert_eq!(roundtrip_f32[0], 0.0);
    assert!((roundtrip_f32[1] - 0.5).abs() < 1e-4);
    assert!((roundtrip_f32[2] - 1.0).abs() < 1e-4);
    assert!((roundtrip_f32[3] - (-1.0)).abs() < 1e-4);
}

#[test]
fn byte_buffer_conversions_match_wire_spec() {
    let mut original_frame = [0.0_f32; HOP_SAMPLES];
    for (i, sample) in original_frame.iter_mut().enumerate() {
        *sample = (i as f32) / (HOP_SAMPLES as f32);
    }

    // Float32 byte serialization
    let f32_bytes = audio_frame_to_f32_bytes(&original_frame);
    assert_eq!(f32_bytes.len(), HOP_SAMPLES * 4);
    let decoded_frame = f32_bytes_to_audio_frame(&f32_bytes).expect("f32 decode must succeed");
    for i in 0..HOP_SAMPLES {
        assert_eq!(decoded_frame[i], original_frame[i]);
    }

    // PCM16 byte serialization
    let pcm16_bytes = audio_frame_to_pcm16_bytes(&original_frame);
    assert_eq!(pcm16_bytes.len(), HOP_SAMPLES * 2);
    let decoded_pcm_frame =
        pcm16_bytes_to_audio_frame(&pcm16_bytes).expect("pcm16 decode must succeed");
    for i in 0..HOP_SAMPLES {
        assert!((decoded_pcm_frame[i] - original_frame[i]).abs() < 1e-3);
    }

    // Buffer length errors fail closed
    let bad_bytes = [0_u8; 100];
    let err = f32_bytes_to_audio_frame(&bad_bytes).expect_err("truncated buffer must fail");
    assert!(matches!(err, EndpointError::UnsupportedFormat(_)));
}
