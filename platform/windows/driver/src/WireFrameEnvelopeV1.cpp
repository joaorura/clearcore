#include "WireFrameEnvelopeV1.h"

NTSTATUS ValidateEnvelope(
    _In_reads_bytes_(bytes) const WireFrameEnvelopeV1* env,
    _In_ size_t bytes
)
{
    if (env == nullptr) {
        return STATUS_INVALID_PARAMETER;
    }

    // Enforce 8-byte alignment
    if (((ULONG_PTR)env & (WIRE_ENVELOPE_ALIGNMENT - 1)) != 0) {
        return STATUS_DATATYPE_MISALIGNMENT;
    }

    // Exact byte size match
    if (bytes != WIRE_ENVELOPE_TOTAL_BYTES) {
        return STATUS_INVALID_BUFFER_SIZE;
    }

    // Version validation (must be 1)
    if (env->VersionLe != WIRE_ENVELOPE_VERSION) {
        return STATUS_REVISION_MISMATCH;
    }

    // Payload length validation (must be 1920)
    if (env->PayloadLenBytesLe != WIRE_ENVELOPE_PAYLOAD_BYTES) {
        return STATUS_INVALID_BUFFER_SIZE;
    }

    // Reserved field must be 0
    if (env->ReservedLe != 0) {
        return STATUS_INVALID_PARAMETER;
    }

    // Discontinuity flags must only use known bits (0x0F)
    if ((env->FlagsLe & ~DISCONTINUITY_FLAG_KNOWN_BITS) != 0) {
        return STATUS_INVALID_PARAMETER;
    }

    return STATUS_SUCCESS;
}

BOOLEAN EnvelopeHasFlag(
    _In_ const WireFrameEnvelopeV1* env,
    _In_ UINT32 flag
)
{
    if (env == nullptr) {
        return FALSE;
    }
    return (env->FlagsLe & flag) == flag;
}

VOID SanitizeEnvelopeSamples(
    _Inout_ WireFrameEnvelopeV1* env
)
{
    if (env == nullptr) {
        return;
    }

    // Kernel-safe bitwise NaN/Inf detection without requiring FPU/SSE state save
    for (UINT32 i = 0; i < WIRE_ENVELOPE_HOP_SAMPLES; ++i) {
        UINT32 rawBits;
        RtlCopyMemory(&rawBits, &env->Payload.Samples[i], sizeof(UINT32));

        // Exponent bits are all 1s (0x7F800000) for NaN and Inf
        if ((rawBits & 0x7F800000U) == 0x7F800000U) {
            // Replace invalid float with digital silence (0.0f)
            FLOAT zero = 0.0f;
            RtlCopyMemory(&env->Payload.Samples[i], &zero, sizeof(FLOAT));
        }
    }
}
