use core::fmt;
use core::mem::{align_of, offset_of, size_of};

use crate::{AudioFrame, Discontinuity, FrameEnvelope, HOP_SAMPLES};

const VERSION: u32 = 1;
const PAYLOAD_LEN_BYTES: u32 = 1_920;
const WIRE_SIZE_BYTES: usize = 1_960;
const SAMPLES_OFFSET: usize = 40;
const SAMPLE_SIZE_BYTES: usize = size_of::<f32>();

#[repr(C, align(8))]
#[allow(
    clippy::struct_field_names,
    reason = "field names mirror the binding wire layout"
)]
pub struct WireFrameEnvelopeV1 {
    version_le: u32,
    payload_len_bytes_le: u32,
    flags_le: u32,
    reserved_le: u32,
    sequence_le: u64,
    capture_monotonic_ns_le: u64,
    generation_le: u64,
    samples_le: [u8; PAYLOAD_LEN_BYTES as usize],
}

const _: () = assert!(size_of::<WireFrameEnvelopeV1>() == WIRE_SIZE_BYTES);
const _: () = assert!(align_of::<WireFrameEnvelopeV1>() == 8);
const _: () = assert!(offset_of!(WireFrameEnvelopeV1, version_le) == 0);
const _: () = assert!(offset_of!(WireFrameEnvelopeV1, payload_len_bytes_le) == 4);
const _: () = assert!(offset_of!(WireFrameEnvelopeV1, flags_le) == 8);
const _: () = assert!(offset_of!(WireFrameEnvelopeV1, reserved_le) == 12);
const _: () = assert!(offset_of!(WireFrameEnvelopeV1, sequence_le) == 16);
const _: () = assert!(offset_of!(WireFrameEnvelopeV1, capture_monotonic_ns_le) == 24);
const _: () = assert!(offset_of!(WireFrameEnvelopeV1, generation_le) == 32);
const _: () = assert!(offset_of!(WireFrameEnvelopeV1, samples_le) == SAMPLES_OFFSET);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WireDecodeError {
    InvalidLength { actual: usize },
    UnsupportedVersion { actual: u32 },
    InvalidPayloadLength { actual: u32 },
    UnknownFlags { actual: u32 },
    NonZeroReserved { actual: u32 },
}

impl fmt::Display for WireDecodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLength { actual } => {
                write!(
                    formatter,
                    "wire frame length {actual} is not {WIRE_SIZE_BYTES}"
                )
            }
            Self::UnsupportedVersion { actual } => {
                write!(formatter, "wire frame version {actual} is not {VERSION}")
            }
            Self::InvalidPayloadLength { actual } => write!(
                formatter,
                "wire frame payload length {actual} is not {PAYLOAD_LEN_BYTES}"
            ),
            Self::UnknownFlags { actual } => {
                write!(
                    formatter,
                    "wire frame flags {actual:#010x} contain unknown bits"
                )
            }
            Self::NonZeroReserved { actual } => {
                write!(formatter, "wire frame reserved field is {actual:#010x}")
            }
        }
    }
}

impl std::error::Error for WireDecodeError {}

impl WireFrameEnvelopeV1 {
    /// Encodes one envelope into the fixed version 1 wire record.
    pub fn encode(envelope: &FrameEnvelope) -> [u8; WIRE_SIZE_BYTES] {
        let mut bytes = [0_u8; WIRE_SIZE_BYTES];
        bytes[0..4].copy_from_slice(&VERSION.to_le_bytes());
        bytes[4..8].copy_from_slice(&PAYLOAD_LEN_BYTES.to_le_bytes());
        bytes[8..12].copy_from_slice(&u32::from(envelope.discontinuity.bits()).to_le_bytes());
        bytes[12..16].copy_from_slice(&0_u32.to_le_bytes());
        bytes[16..24].copy_from_slice(&envelope.sequence.to_le_bytes());
        bytes[24..32].copy_from_slice(&envelope.capture_monotonic_ns.to_le_bytes());
        bytes[32..40].copy_from_slice(&envelope.generation.to_le_bytes());
        for (encoded, sample) in bytes[SAMPLES_OFFSET..]
            .chunks_exact_mut(SAMPLE_SIZE_BYTES)
            .zip(envelope.samples)
        {
            encoded.copy_from_slice(&sample.to_le_bytes());
        }
        bytes
    }

    /// Decodes one checked version 1 wire record.
    ///
    /// # Errors
    ///
    /// Returns a typed error when the record header violates the version 1 contract.
    pub fn decode(bytes: &[u8]) -> Result<FrameEnvelope, WireDecodeError> {
        if bytes.len() != WIRE_SIZE_BYTES {
            return Err(WireDecodeError::InvalidLength {
                actual: bytes.len(),
            });
        }

        let version = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        if version != VERSION {
            return Err(WireDecodeError::UnsupportedVersion { actual: version });
        }

        let payload_len = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        if payload_len != PAYLOAD_LEN_BYTES {
            return Err(WireDecodeError::InvalidPayloadLength {
                actual: payload_len,
            });
        }

        let raw_flags = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
        if raw_flags & !u32::from(Discontinuity::KNOWN_BITS) != 0 {
            return Err(WireDecodeError::UnknownFlags { actual: raw_flags });
        }
        let flags = u8::try_from(raw_flags)
            .map_err(|_| WireDecodeError::UnknownFlags { actual: raw_flags })?;
        let discontinuity = Discontinuity::from_bits(flags)
            .ok_or(WireDecodeError::UnknownFlags { actual: raw_flags })?;

        let reserved = u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]);
        if reserved != 0 {
            return Err(WireDecodeError::NonZeroReserved { actual: reserved });
        }

        let sequence = u64::from_le_bytes([
            bytes[16], bytes[17], bytes[18], bytes[19], bytes[20], bytes[21], bytes[22], bytes[23],
        ]);
        let capture_monotonic_ns = u64::from_le_bytes([
            bytes[24], bytes[25], bytes[26], bytes[27], bytes[28], bytes[29], bytes[30], bytes[31],
        ]);
        let generation = u64::from_le_bytes([
            bytes[32], bytes[33], bytes[34], bytes[35], bytes[36], bytes[37], bytes[38], bytes[39],
        ]);
        let mut samples: AudioFrame = [0.0; HOP_SAMPLES];
        for (sample, encoded) in samples
            .iter_mut()
            .zip(bytes[SAMPLES_OFFSET..].chunks_exact(SAMPLE_SIZE_BYTES))
        {
            *sample = f32::from_le_bytes([encoded[0], encoded[1], encoded[2], encoded[3]]);
        }

        Ok(FrameEnvelope {
            samples,
            sequence,
            capture_monotonic_ns,
            generation,
            discontinuity,
        })
    }
}
