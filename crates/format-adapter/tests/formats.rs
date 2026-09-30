#![forbid(unsafe_code)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::float_cmp)]

use realtime_noise_format_adapter::{
    AudioEndpointConfig, EndpointFormatConverter, FormatError, SampleFormat,
};

#[test]
fn unsupported_endpoint_format_fails_instead_of_converting_implicitly() {
    // Unsupported channel count (e.g. 5.1 surround / 6 channels, 0 channels, or 3 channels)
    let config_surround = AudioEndpointConfig::new(48_000, 6, SampleFormat::Float32);
    let result = EndpointFormatConverter::validate_config(&config_surround);
    assert_eq!(result, Err(FormatError::UnsupportedChannels(6)));

    let config_zero = AudioEndpointConfig::new(48_000, 0, SampleFormat::Float32);
    let result_zero = EndpointFormatConverter::validate_config(&config_zero);
    assert_eq!(result_zero, Err(FormatError::UnsupportedChannels(0)));

    let config_three = AudioEndpointConfig::new(48_000, 3, SampleFormat::Float32);
    let result_three = EndpointFormatConverter::validate_config(&config_three);
    assert_eq!(result_three, Err(FormatError::UnsupportedChannels(3)));

    // Unsupported sample rate (e.g. 44.1 kHz)
    let config_cd_rate = AudioEndpointConfig::new(44_100, 1, SampleFormat::Float32);
    let result_rate = EndpointFormatConverter::validate_config(&config_cd_rate);
    assert_eq!(result_rate, Err(FormatError::UnsupportedSampleRate(44_100)));

    // Buffer length mismatch must fail explicitly
    let i16_in = [0i16; 10];
    let mut f32_out = [0.0f32; 5];
    let mismatch_result = EndpointFormatConverter::pcm16_to_f32(&i16_in, &mut f32_out);
    assert_eq!(
        mismatch_result,
        Err(FormatError::BufferLengthMismatch {
            expected: 10,
            actual: 5
        })
    );

    let stereo_in = [0.0f32; 8];
    let mut mono_out = [0.0f32; 3]; // expected 4
    let stereo_mismatch = EndpointFormatConverter::stereo_to_mono(&stereo_in, &mut mono_out);
    assert_eq!(
        stereo_mismatch,
        Err(FormatError::BufferLengthMismatch {
            expected: 4,
            actual: 3
        })
    );
}

#[test]
fn pcm16_conversion_roundtrip_with_saturation() {
    // Test conversion between f32 and i16 with saturation clamping
    let f32_inputs = [
        0.0f32, 0.5, -0.5, 1.0, -1.0, 2.5,  // should saturate to i16::MAX (32767)
        -3.0, // should saturate to i16::MIN (-32768)
    ];

    let mut i16_buffer = [0i16; 7];
    EndpointFormatConverter::f32_to_pcm16(&f32_inputs, &mut i16_buffer)
        .expect("f32 to pcm16 conversion should succeed");

    assert_eq!(i16_buffer[0], 0);
    assert_eq!(i16_buffer[3], 32767);
    assert_eq!(i16_buffer[4], -32768);
    assert_eq!(i16_buffer[5], 32767, "values > 1.0 must saturate to 32767");
    assert_eq!(
        i16_buffer[6], -32768,
        "values < -1.0 must saturate to -32768"
    );

    // Convert back from i16 to f32
    let mut f32_outputs = [0.0f32; 7];
    EndpointFormatConverter::pcm16_to_f32(&i16_buffer, &mut f32_outputs)
        .expect("pcm16 to f32 conversion should succeed");

    // Zero, +1.0, -1.0 must match exactly
    assert_eq!(f32_outputs[0], 0.0);
    assert_eq!(f32_outputs[3], 1.0);
    assert_eq!(f32_outputs[4], -1.0);
    assert_eq!(f32_outputs[5], 1.0);
    assert_eq!(f32_outputs[6], -1.0);

    // Intermediate values must be accurate within 1 LSB (1.0 / 32767.0)
    let tolerance = (1.0 / 32767.0) + 1e-6;
    assert!((f32_outputs[1] - 0.5).abs() <= tolerance);
    assert!((f32_outputs[2] - (-0.5)).abs() <= tolerance);
}

#[test]
fn stereo_to_mono_and_mono_to_stereo_roundtrip() {
    // Left: 0.2, Right: 0.8 => mono average should be 0.5
    // Left: -0.4, Right: -0.6 => mono average should be -0.5
    let stereo_in = [0.2f32, 0.8, -0.4, -0.6];
    let mut mono_out = [0.0f32; 2];

    EndpointFormatConverter::stereo_to_mono(&stereo_in, &mut mono_out)
        .expect("stereo to mono conversion should succeed");

    assert!((mono_out[0] - 0.5).abs() < 1e-6);
    assert!((mono_out[1] - (-0.5)).abs() < 1e-6);

    // Now upmix mono back to stereo: mono [0.5, -0.5] -> stereo [0.5, 0.5, -0.5, -0.5]
    let mut stereo_out = [0.0f32; 4];
    EndpointFormatConverter::mono_to_stereo(&mono_out, &mut stereo_out)
        .expect("mono to stereo conversion should succeed");

    assert_eq!(stereo_out, [0.5, 0.5, -0.5, -0.5]);
}
