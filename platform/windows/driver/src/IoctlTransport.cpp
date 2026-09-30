#include "IoctlTransport.h"

CIoctlTransport::CIoctlTransport()
    : m_SessionActive(FALSE),
      m_ActiveSessionId(0),
      m_ActiveProcessId(nullptr),
      m_ActiveFileObject(nullptr),
      m_ActiveGeneration(0),
      m_LastSequence(0),
      m_HasFirstSequence(FALSE),
      m_QueueReadIndex(0),
      m_QueueWriteIndex(0),
      m_QueueCount(0)
{
    RtlZeroMemory(&m_Lock, sizeof(m_Lock));
    RtlZeroMemory(&m_Stats, sizeof(m_Stats));
    RtlZeroMemory(m_Queue, sizeof(m_Queue));
}

CIoctlTransport::~CIoctlTransport()
{
    Reset();
}

NTSTATUS CIoctlTransport::Initialize()
{
    KeInitializeSpinLock(&m_Lock);
    Reset();
    return STATUS_SUCCESS;
}

VOID CIoctlTransport::Reset()
{
    KIRQL oldIrql;
    KeAcquireSpinLock(&m_Lock, &oldIrql);

    m_SessionActive = FALSE;
    m_ActiveSessionId = 0;
    m_ActiveProcessId = nullptr;
    m_ActiveFileObject = nullptr;
    m_ActiveGeneration = 0;
    m_LastSequence = 0;
    m_HasFirstSequence = FALSE;
    m_QueueReadIndex = 0;
    m_QueueWriteIndex = 0;
    m_QueueCount = 0;

    m_Stats.IsSessionActive = FALSE;
    m_Stats.ActiveSessionId = 0;
    m_Stats.ActiveProcessId = 0;

    RtlZeroMemory(m_Queue, sizeof(m_Queue));

    KeReleaseSpinLock(&m_Lock, oldIrql);
}

NTSTATUS CIoctlTransport::AcquireSession(
    _In_ PFILE_OBJECT fileObject,
    _In_ ULONG sessionId,
    _In_ HANDLE processId
)
{
    if (fileObject == nullptr) {
        return STATUS_INVALID_PARAMETER;
    }

    KIRQL oldIrql;
    KeAcquireSpinLock(&m_Lock, &oldIrql);

    if (m_SessionActive) {
        // Check if caller is already the session owner
        if (m_ActiveFileObject == fileObject && m_ActiveSessionId == sessionId) {
            KeReleaseSpinLock(&m_Lock, oldIrql);
            return STATUS_SUCCESS;
        }

        // Conflicting session: report device busy (UI maps to UnavailableBusy)
        KeReleaseSpinLock(&m_Lock, oldIrql);
        return STATUS_DEVICE_BUSY;
    }

    m_SessionActive = TRUE;
    m_ActiveFileObject = fileObject;
    m_ActiveSessionId = sessionId;
    m_ActiveProcessId = processId;
    m_HasFirstSequence = FALSE;
    m_QueueReadIndex = 0;
    m_QueueWriteIndex = 0;
    m_QueueCount = 0;

    m_Stats.IsSessionActive = TRUE;
    m_Stats.ActiveSessionId = sessionId;
    m_Stats.ActiveProcessId = HandleToUlong(processId);

    KeReleaseSpinLock(&m_Lock, oldIrql);
    return STATUS_SUCCESS;
}

NTSTATUS CIoctlTransport::ReleaseSession(
    _In_ PFILE_OBJECT fileObject
)
{
    if (fileObject == nullptr) {
        return STATUS_INVALID_PARAMETER;
    }

    KIRQL oldIrql;
    KeAcquireSpinLock(&m_Lock, &oldIrql);

    if (!m_SessionActive) {
        KeReleaseSpinLock(&m_Lock, oldIrql);
        return STATUS_SUCCESS;
    }

    if (m_ActiveFileObject != fileObject) {
        KeReleaseSpinLock(&m_Lock, oldIrql);
        return STATUS_ACCESS_DENIED;
    }

    m_SessionActive = FALSE;
    m_ActiveFileObject = nullptr;
    m_ActiveSessionId = 0;
    m_ActiveProcessId = nullptr;
    m_HasFirstSequence = FALSE;
    m_QueueReadIndex = 0;
    m_QueueWriteIndex = 0;
    m_QueueCount = 0;

    m_Stats.IsSessionActive = FALSE;
    m_Stats.ActiveSessionId = 0;
    m_Stats.ActiveProcessId = 0;

    KeReleaseSpinLock(&m_Lock, oldIrql);
    return STATUS_SUCCESS;
}

BOOLEAN CIoctlTransport::IsSessionOwner(
    _In_ PFILE_OBJECT fileObject,
    _In_ ULONG sessionId,
    _In_ HANDLE processId
)
{
    UNREFERENCED_PARAMETER(processId);
    KIRQL oldIrql;
    KeAcquireSpinLock(&m_Lock, &oldIrql);

    BOOLEAN isOwner = (m_SessionActive &&
                       m_ActiveFileObject == fileObject &&
                       m_ActiveSessionId == sessionId);

    KeReleaseSpinLock(&m_Lock, oldIrql);
    return isOwner;
}

VOID CIoctlTransport::HandleFileCleanup(
    _In_ PFILE_OBJECT fileObject
)
{
    if (fileObject == nullptr) {
        return;
    }

    KIRQL oldIrql;
    KeAcquireSpinLock(&m_Lock, &oldIrql);

    if (m_SessionActive && m_ActiveFileObject == fileObject) {
        m_SessionActive = FALSE;
        m_ActiveFileObject = nullptr;
        m_ActiveSessionId = 0;
        m_ActiveProcessId = nullptr;
        m_HasFirstSequence = FALSE;
        m_QueueReadIndex = 0;
        m_QueueWriteIndex = 0;
        m_QueueCount = 0;

        m_Stats.IsSessionActive = FALSE;
        m_Stats.ActiveSessionId = 0;
        m_Stats.ActiveProcessId = 0;
    }

    KeReleaseSpinLock(&m_Lock, oldIrql);
}

NTSTATUS CIoctlTransport::SubmitEnvelope(
    _In_ PFILE_OBJECT fileObject,
    _In_ ULONG sessionId,
    _In_ HANDLE processId,
    _In_reads_bytes_(bytes) const WireFrameEnvelopeV1* envelope,
    _In_ size_t bytes
)
{
    // 1. Validate envelope wire contract (size, alignment, version, reserved, flags)
    NTSTATUS status = ValidateEnvelope(envelope, bytes);
    if (!NT_SUCCESS(status)) {
        return status;
    }

    KIRQL oldIrql;
    KeAcquireSpinLock(&m_Lock, &oldIrql);

    m_Stats.TotalEnvelopesSubmitted++;

    // 2. Validate session ownership
    if (m_SessionActive) {
        if (m_ActiveFileObject != fileObject || m_ActiveSessionId != sessionId) {
            m_Stats.TotalEnvelopesDropped++;
            KeReleaseSpinLock(&m_Lock, oldIrql);
            return STATUS_ACCESS_DENIED;
        }
    } else {
        // Auto-acquire on first envelope submission
        m_SessionActive = TRUE;
        m_ActiveFileObject = fileObject;
        m_ActiveSessionId = sessionId;
        m_ActiveProcessId = processId;
        m_Stats.IsSessionActive = TRUE;
        m_Stats.ActiveSessionId = sessionId;
        m_Stats.ActiveProcessId = HandleToUlong(processId);
    }

    // 3. Generation and Monotonic Sequence Verification
    const UINT64 gen = envelope->GenerationLe;
    const UINT64 seq = envelope->SequenceLe;

    if (!m_HasFirstSequence) {
        m_ActiveGeneration = gen;
        m_LastSequence = seq;
        m_HasFirstSequence = TRUE;
    } else {
        if (gen < m_ActiveGeneration) {
            // Stale generation from previous stream; reject
            m_Stats.TotalEnvelopesDropped++;
            KeReleaseSpinLock(&m_Lock, oldIrql);
            return STATUS_DATA_LATE_ERROR;
        } else if (gen > m_ActiveGeneration) {
            // Stream restart or generation bump; flush stale queue and accept new generation
            m_ActiveGeneration = gen;
            m_LastSequence = seq;
            m_QueueReadIndex = 0;
            m_QueueWriteIndex = 0;
            m_QueueCount = 0;
        } else {
            // Same generation: sequence must monotonically advance
            if (seq <= m_LastSequence) {
                m_Stats.TotalEnvelopesDropped++;
                KeReleaseSpinLock(&m_Lock, oldIrql);
                return STATUS_DATA_LATE_ERROR;
            }
            m_LastSequence = seq;
        }
    }

    // 4. Enqueue into staging circular buffer
    if (m_QueueCount >= TRANSPORT_QUEUE_CAPACITY) {
        // Evict oldest hop to prevent unbounded latency drift
        m_QueueReadIndex = (m_QueueReadIndex + 1) % TRANSPORT_QUEUE_CAPACITY;
        m_QueueCount--;
        m_Stats.TotalEnvelopesDropped++;
    }

    WireFrameEnvelopeV1* slot = &m_Queue[m_QueueWriteIndex];
    RtlCopyMemory(slot, envelope, sizeof(WireFrameEnvelopeV1));
    SanitizeEnvelopeSamples(slot);

    m_QueueWriteIndex = (m_QueueWriteIndex + 1) % TRANSPORT_QUEUE_CAPACITY;
    m_QueueCount++;

    m_Stats.TotalEnvelopesAccepted++;
    m_Stats.ActiveGeneration = m_ActiveGeneration;
    m_Stats.LastSequence = m_LastSequence;

    KeReleaseSpinLock(&m_Lock, oldIrql);
    return STATUS_SUCCESS;
}

BOOLEAN CIoctlTransport::ConsumeSamples(
    _Out_writes_(sampleCount) FLOAT* outSamples,
    _In_ UINT32 sampleCount
)
{
    if (outSamples == nullptr || sampleCount == 0) {
        return FALSE;
    }

    KIRQL oldIrql;
    KeAcquireSpinLock(&m_Lock, &oldIrql);

    // Fail-closed policy: if session not active or queue empty, report underrun
    if (!m_SessionActive || m_QueueCount == 0) {
        m_Stats.UnderrunCount++;
        KeReleaseSpinLock(&m_Lock, oldIrql);
        return FALSE;
    }

    WireFrameEnvelopeV1* envelope = &m_Queue[m_QueueReadIndex];
    const UINT32 copyCount = (sampleCount < WIRE_ENVELOPE_HOP_SAMPLES) ? sampleCount : WIRE_ENVELOPE_HOP_SAMPLES;

    RtlCopyMemory(outSamples, envelope->Payload.Samples, copyCount * sizeof(FLOAT));

    // Zero out any remaining caller requested samples beyond hop size
    if (copyCount < sampleCount) {
        RtlZeroMemory(&outSamples[copyCount], (sampleCount - copyCount) * sizeof(FLOAT));
    }

    m_QueueReadIndex = (m_QueueReadIndex + 1) % TRANSPORT_QUEUE_CAPACITY;
    m_QueueCount--;

    KeReleaseSpinLock(&m_Lock, oldIrql);
    return TRUE;
}

VOID CIoctlTransport::GetStats(
    _Out_ TransportStats* stats
)
{
    if (stats == nullptr) {
        return;
    }

    KIRQL oldIrql;
    KeAcquireSpinLock(&m_Lock, &oldIrql);
    RtlCopyMemory(stats, &m_Stats, sizeof(TransportStats));
    KeReleaseSpinLock(&m_Lock, oldIrql);
}
