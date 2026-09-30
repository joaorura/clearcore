#pragma once

#include <ntdef.h>
#include <ntstatus.h>

#define WIRE_ENVELOPE_VERSION                 1U
#define WIRE_ENVELOPE_PAYLOAD_BYTES          1920U
#define WIRE_ENVELOPE_TOTAL_BYTES            1960U
#define WIRE_ENVELOPE_HOP_SAMPLES             480U
#define WIRE_ENVELOPE_SAMPLE_RATE_HZ        48000U
#define WIRE_ENVELOPE_CHANNELS                  1U
#define WIRE_ENVELOPE_ALIGNMENT                 8U

// Discontinuity flags matching crates/contracts/src/audio.rs
#define DISCONTINUITY_FLAG_NONE                    0x00U
#define DISCONTINUITY_FLAG_CAPTURE_DROP            0x01U
#define DISCONTINUITY_FLAG_DEVICE_CHANGE           0x02U
#define DISCONTINUITY_FLAG_GENERATION_CHANGE       0x04U
#define DISCONTINUITY_FLAG_INFERENCE_DEADLINE_MISS 0x08U
#define DISCONTINUITY_FLAG_KNOWN_BITS              0x0FU

#pragma pack(push, 8)
struct alignas(8) WireFrameEnvelopeV1 {
    UINT32 VersionLe;             // Offset 0: Must equal WIRE_ENVELOPE_VERSION (1)
    UINT32 PayloadLenBytesLe;     // Offset 4: Must equal WIRE_ENVELOPE_PAYLOAD_BYTES (1920)
    UINT32 FlagsLe;               // Offset 8: Discontinuity flags (bits 0..3)
    UINT32 ReservedLe;            // Offset 12: Must equal 0
    UINT64 SequenceLe;            // Offset 16: Monotonically increasing sequence number
    UINT64 CaptureMonotonicNsLe;  // Offset 24: Monotonic capture timestamp in nanoseconds
    UINT64 GenerationLe;          // Offset 32: Generation counter for stream resets
    union {
        FLOAT  Samples[WIRE_ENVELOPE_HOP_SAMPLES]; // Offset 40: 480 IEEE-754 32-bit floats
        UINT8  RawBytes[WIRE_ENVELOPE_PAYLOAD_BYTES];
    } Payload;
};
#pragma pack(pop)

// Compile-time layout verification matching crates/contracts/src/wire.rs
static_assert(sizeof(WireFrameEnvelopeV1) == WIRE_ENVELOPE_TOTAL_BYTES,
              "WireFrameEnvelopeV1 size must be exactly 1960 bytes");
static_assert(alignof(WireFrameEnvelopeV1) == WIRE_ENVELOPE_ALIGNMENT,
              "WireFrameEnvelopeV1 must have 8-byte alignment");
static_assert(FIELD_OFFSET(WireFrameEnvelopeV1, VersionLe) == 0,
              "WireFrameEnvelopeV1::VersionLe offset mismatch");
static_assert(FIELD_OFFSET(WireFrameEnvelopeV1, PayloadLenBytesLe) == 4,
              "WireFrameEnvelopeV1::PayloadLenBytesLe offset mismatch");
static_assert(FIELD_OFFSET(WireFrameEnvelopeV1, FlagsLe) == 8,
              "WireFrameEnvelopeV1::FlagsLe offset mismatch");
static_assert(FIELD_OFFSET(WireFrameEnvelopeV1, ReservedLe) == 12,
              "WireFrameEnvelopeV1::ReservedLe offset mismatch");
static_assert(FIELD_OFFSET(WireFrameEnvelopeV1, SequenceLe) == 16,
              "WireFrameEnvelopeV1::SequenceLe offset mismatch");
static_assert(FIELD_OFFSET(WireFrameEnvelopeV1, CaptureMonotonicNsLe) == 24,
              "WireFrameEnvelopeV1::CaptureMonotonicNsLe offset mismatch");
static_assert(FIELD_OFFSET(WireFrameEnvelopeV1, GenerationLe) == 32,
              "WireFrameEnvelopeV1::GenerationLe offset mismatch");
static_assert(FIELD_OFFSET(WireFrameEnvelopeV1, Payload) == 40,
              "WireFrameEnvelopeV1::Payload offset mismatch");

/**
 * Validates a WireFrameEnvelopeV1 buffer received from user-mode callers.
 * Ensures zero-leakage, alignment, bounds, versioning, and flag integrity.
 */
NTSTATUS ValidateEnvelope(
    _In_reads_bytes_(bytes) const WireFrameEnvelopeV1* env,
    _In_ size_t bytes
);

/**
 * Checks whether an envelope contains a specific discontinuity flag.
 */
BOOLEAN EnvelopeHasFlag(
    _In_ const WireFrameEnvelopeV1* env,
    _In_ UINT32 flag
);

/**
 * Sanitizes envelope samples, clamping NaN/Inf floats to digital silence (0.0f).
 */
VOID SanitizeEnvelopeSamples(
    _Inout_ WireFrameEnvelopeV1* env
);
