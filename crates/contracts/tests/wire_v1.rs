use realtime_noise_contracts::{
    AudioFrame, CHANNELS, Discontinuity, FrameEnvelope, HOP_SAMPLES, SAMPLE_RATE_HZ,
    WireDecodeError, WireFrameEnvelopeV1,
};

const VALID_FIXTURE: &[u8; 1_960] = include_bytes!("../../../fixtures/wire/frame-v1-valid.bin");
const INVALID_RESERVED_FIXTURE: &[u8; 1_960] =
    include_bytes!("../../../fixtures/wire/frame-v1-invalid-reserved.bin");

fn fixture_envelope() -> FrameEnvelope {
    let mut samples: AudioFrame = [0.0; HOP_SAMPLES];
    samples[0] = 0.0;
    samples[1] = -0.0;
    samples[2] = 1.0;
    samples[3] = -1.0;
    FrameEnvelope {
        samples,
        sequence: 42,
        capture_monotonic_ns: 1_000_000_000,
        generation: 7,
        discontinuity: Discontinuity::CAPTURE_DROP | Discontinuity::GENERATION_CHANGE,
    }
}

#[test]
fn audio_contract_uses_fixed_mono_ten_millisecond_hops() {
    let samples: AudioFrame = [0.0; HOP_SAMPLES];

    assert_eq!(SAMPLE_RATE_HZ, 48_000);
    assert_eq!(CHANNELS, 1);
    assert_eq!(samples.len(), 480);
}

#[test]
fn frame_envelope_is_directly_constructible_and_copyable() {
    let envelope = fixture_envelope();

    let copied = envelope;

    assert_eq!(envelope.sequence, copied.sequence);
    assert_eq!(envelope.discontinuity.bits(), copied.discontinuity.bits());
}

#[test]
fn discontinuity_uses_u8_bits_and_typed_contains() {
    let combined = Discontinuity::CAPTURE_DROP | Discontinuity::GENERATION_CHANGE;

    let bits: u8 = combined.bits();

    assert_eq!(Discontinuity::NONE.bits(), 0);
    assert_eq!(Discontinuity::CAPTURE_DROP.bits(), 1);
    assert_eq!(Discontinuity::DEVICE_CHANGE.bits(), 2);
    assert_eq!(Discontinuity::GENERATION_CHANGE.bits(), 4);
    assert_eq!(Discontinuity::INFERENCE_DEADLINE_MISS.bits(), 8);
    assert_eq!(bits, 5);
    assert!(combined.contains(Discontinuity::CAPTURE_DROP));
    assert!(!combined.contains(Discontinuity::DEVICE_CHANGE));
    assert_eq!(Discontinuity::from_bits(bits), Some(combined));
    assert!(Discontinuity::from_bits(16_u8).is_none());
}

#[test]
fn wire_v1_rejects_malformed_total_lengths() {
    let short = &VALID_FIXTURE[..1_959];
    let mut long = [0_u8; 1_961];
    long[..1_960].copy_from_slice(VALID_FIXTURE);

    assert_eq!(
        WireFrameEnvelopeV1::decode(short).err(),
        Some(WireDecodeError::InvalidLength { actual: 1_959 })
    );
    assert_eq!(
        WireFrameEnvelopeV1::decode(&long).err(),
        Some(WireDecodeError::InvalidLength { actual: 1_961 })
    );
}

#[test]
fn wire_v1_rejects_unsupported_version() {
    let mut bytes = *VALID_FIXTURE;
    bytes[0..4].copy_from_slice(&2_u32.to_le_bytes());

    assert_eq!(
        WireFrameEnvelopeV1::decode(&bytes).err(),
        Some(WireDecodeError::UnsupportedVersion { actual: 2 })
    );
}

#[test]
fn wire_v1_rejects_wrong_payload_size() {
    let mut bytes = *VALID_FIXTURE;
    bytes[4..8].copy_from_slice(&1_916_u32.to_le_bytes());

    assert_eq!(
        WireFrameEnvelopeV1::decode(&bytes).err(),
        Some(WireDecodeError::InvalidPayloadLength { actual: 1_916 })
    );
}

#[test]
fn wire_v1_rejects_low_unknown_flag() {
    let mut bytes = *VALID_FIXTURE;
    bytes[8..12].copy_from_slice(&16_u32.to_le_bytes());

    assert_eq!(
        WireFrameEnvelopeV1::decode(&bytes).err(),
        Some(WireDecodeError::UnknownFlags { actual: 16 })
    );
}

#[test]
fn wire_v1_rejects_high_unknown_flag() {
    let mut bytes = *VALID_FIXTURE;
    bytes[8..12].copy_from_slice(&(1_u32 << 31).to_le_bytes());

    assert_eq!(
        WireFrameEnvelopeV1::decode(&bytes).err(),
        Some(WireDecodeError::UnknownFlags {
            actual: 1_u32 << 31,
        })
    );
}

#[test]
fn wire_v1_rejects_each_nonzero_reserved_byte() {
    for reserved_offset in 12..16 {
        let mut bytes = *VALID_FIXTURE;
        bytes[reserved_offset] = 1;

        assert!(matches!(
            WireFrameEnvelopeV1::decode(&bytes),
            Err(WireDecodeError::NonZeroReserved { .. })
        ));
    }
}

#[test]
fn wire_v1_preserves_signed_zero_infinity_and_nan_sample_bits() -> Result<(), WireDecodeError> {
    let mut envelope = fixture_envelope();
    let sample_bits = [
        0x0000_0000_u32,
        0x8000_0000,
        0x7f80_0000,
        0xff80_0000,
        0x7fc0_1234,
        0x7f80_0001,
    ];
    for (sample, bits) in envelope.samples.iter_mut().zip(sample_bits) {
        *sample = f32::from_bits(bits);
    }

    let round_tripped = WireFrameEnvelopeV1::decode(&WireFrameEnvelopeV1::encode(&envelope))?;

    for (sample, expected_bits) in round_tripped.samples.iter().zip(sample_bits) {
        assert_eq!(sample.to_bits(), expected_bits);
    }
    Ok(())
}

#[test]
fn valid_fixture_decodes_and_reencodes_exactly() -> Result<(), WireDecodeError> {
    let envelope = WireFrameEnvelopeV1::decode(VALID_FIXTURE)?;

    assert_eq!(envelope.sequence, 42);
    assert_eq!(envelope.capture_monotonic_ns, 1_000_000_000);
    assert_eq!(envelope.generation, 7);
    assert_eq!(envelope.discontinuity.bits(), 5);
    assert_eq!(envelope.samples[0].to_bits(), 0.0_f32.to_bits());
    assert_eq!(envelope.samples[1].to_bits(), (-0.0_f32).to_bits());
    assert_eq!(envelope.samples[2].to_bits(), 1.0_f32.to_bits());
    assert_eq!(envelope.samples[3].to_bits(), (-1.0_f32).to_bits());
    assert_eq!(WireFrameEnvelopeV1::encode(&envelope), *VALID_FIXTURE);
    Ok(())
}

#[test]
fn checked_invalid_fixture_rejects_nonzero_reserved_field() {
    assert_eq!(
        WireFrameEnvelopeV1::decode(INVALID_RESERVED_FIXTURE).err(),
        Some(WireDecodeError::NonZeroReserved { actual: 1 })
    );
}
