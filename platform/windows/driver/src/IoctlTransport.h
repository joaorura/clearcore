#pragma once

#include <ntddk.h>
#include "WireFrameEnvelopeV1.h"

//
// Device Type & IOCTL Definitions
//
#define FILE_DEVICE_REALTIME_NOISE 0x00008A01

#ifndef METHOD_DIRECT_TO_DEVICE
#define METHOD_DIRECT_TO_DEVICE METHOD_IN_DIRECT
#endif

// Submit a single WireFrameEnvelopeV1 via Direct I/O (MDL-backed)
#define IOCTL_REALTIME_NOISE_SUBMIT_ENVELOPE \
    CTL_CODE(FILE_DEVICE_REALTIME_NOISE, 0x801, METHOD_DIRECT_TO_DEVICE, FILE_READ_ACCESS | FILE_WRITE_ACCESS)

// Acquire session ownership for the calling process/session
#define IOCTL_REALTIME_NOISE_ACQUIRE_SESSION \
    CTL_CODE(FILE_DEVICE_REALTIME_NOISE, 0x802, METHOD_BUFFERED, FILE_READ_ACCESS | FILE_WRITE_ACCESS)

// Explicitly release session ownership
#define IOCTL_REALTIME_NOISE_RELEASE_SESSION \
    CTL_CODE(FILE_DEVICE_REALTIME_NOISE, 0x803, METHOD_BUFFERED, FILE_READ_ACCESS | FILE_WRITE_ACCESS)

// Query driver transport status and metrics
#define IOCTL_REALTIME_NOISE_GET_STATS \
    CTL_CODE(FILE_DEVICE_REALTIME_NOISE, 0x804, METHOD_BUFFERED, FILE_READ_ACCESS)

#pragma pack(push, 8)
struct TransportStats {
    UINT64 TotalEnvelopesSubmitted;
    UINT64 TotalEnvelopesAccepted;
    UINT64 TotalEnvelopesDropped;
    UINT64 UnderrunCount;
    UINT64 ActiveGeneration;
    UINT64 LastSequence;
    ULONG  ActiveSessionId;
    ULONG  ActiveProcessId;
    BOOLEAN IsSessionActive;
    UINT8  Reserved[3];
    ULONG  ActiveStreamsCount;
};
#pragma pack(pop)

static_assert(sizeof(TransportStats) == 64, "TransportStats size must be 64 bytes");

// Circular buffer capacity for intermediate envelope staging (16 hops = 160ms)
#define TRANSPORT_QUEUE_CAPACITY 16U

class CIoctlTransport {
private:
    KSPIN_LOCK          m_Lock;
    BOOLEAN             m_SessionActive;
    ULONG               m_ActiveSessionId;
    HANDLE              m_ActiveProcessId;
    PFILE_OBJECT        m_ActiveFileObject;

    UINT64              m_ActiveGeneration;
    UINT64              m_LastSequence;
    BOOLEAN             m_HasFirstSequence;

    // Intermediate FIFO for WaveRT consumer
    WireFrameEnvelopeV1 m_Queue[TRANSPORT_QUEUE_CAPACITY];
    UINT32              m_QueueReadIndex;
    UINT32              m_QueueWriteIndex;
    UINT32              m_QueueCount;

    // Metrics
    TransportStats      m_Stats;
    volatile LONG       m_ActiveStreamsCount;

public:
    CIoctlTransport();
    ~CIoctlTransport();

    NTSTATUS Initialize();

    // Active streams tracking
    LONG IncrementActiveStreams();
    LONG DecrementActiveStreams();
    LONG GetActiveStreamsCount() const;

    // Session Management
    NTSTATUS AcquireSession(
        _In_ PFILE_OBJECT fileObject,
        _In_ ULONG sessionId,
        _In_ HANDLE processId
    );

    NTSTATUS ReleaseSession(
        _In_ PFILE_OBJECT fileObject
    );

    BOOLEAN IsSessionOwner(
        _In_ PFILE_OBJECT fileObject,
        _In_ ULONG sessionId,
        _In_ HANDLE processId
    );

    VOID HandleFileCleanup(
        _In_ PFILE_OBJECT fileObject
    );

    // Envelope Submission
    NTSTATUS SubmitEnvelope(
        _In_ PFILE_OBJECT fileObject,
        _In_ ULONG sessionId,
        _In_ HANDLE processId,
        _In_reads_bytes_(bytes) const WireFrameEnvelopeV1* envelope,
        _In_ size_t bytes
    );

    // Audio Output / WaveRT Consumer interface
    // Retrieves samples into the WaveRT DMA cyclic buffer.
    // If no session is active or an underrun occurs, returns FALSE and
    // caller MUST fill buffer with digital silence (zeroes).
    BOOLEAN ConsumeSamples(
        _Out_writes_(sampleCount) FLOAT* outSamples,
        _In_ UINT32 sampleCount
    );

    VOID GetStats(
        _Out_ TransportStats* stats
    );

    VOID Reset();
};
