#pragma once

#include <portcls.h>
#include <ks.h>
#include <ksmedia.h>
#include "IoctlTransport.h"

// Forward declarations
class CMiniportWaveRTStream;

// WaveRT Cyclic Buffer Constants (30ms to 100ms typical; let's configure 100ms = 10 hops)
#define WAVERT_CAPTURE_SAMPLE_RATE_HZ     48000U
#define WAVERT_CAPTURE_CHANNELS           1U
#define WAVERT_CAPTURE_HOP_SAMPLES        480U
#define WAVERT_CAPTURE_HOP_DURATION_NS    10000000ULL // 10ms in 100ns units
#define WAVERT_CAPTURE_BUFFER_HOPS        10U

// Format identifiers
enum AudioDataFormat {
    AudioFormat_Float32 = 0,
    AudioFormat_Pcm16   = 1
};

//=============================================================================
// CMiniportWaveRT
//=============================================================================
class CMiniportWaveRT : public IMiniportWaveRT,
                        public CUnknown {
private:
    PPORTWAVERTSTREAM   m_PortStream;
    PDEVICE_OBJECT      m_DeviceObject;
    CIoctlTransport*    m_Transport;

public:
    DECLARE_STD_UNKNOWN();

    CMiniportWaveRT(
        _In_opt_ PUNKNOWN UnknownOuter,
        _In_ CIoctlTransport* Transport
    );
    virtual ~CMiniportWaveRT();

    // IMiniport
    STDMETHODIMP DataRangeIntersection(
        _In_        ULONG           PinId,
        _In_        PKSDATARANGE    ClientDataRange,
        _In_        PKSDATARANGE    MyDataRange,
        _In_        ULONG           OutputBufferLength,
        _Out_writes_bytes_to_opt_(OutputBufferLength, *ResultLength) PVOID ResultData,
        _Out_       PULONG          ResultLength
    );

    STDMETHODIMP GetDescription(
        _Out_       PPCFILTER_DESCRIPTOR* Description
    );

    STDMETHODIMP Init(
        _In_        PUNKNOWN        UnknownAdapter,
        _In_        PRESOURCELIST   ResourceList,
        _In_        PPORTWAVERT     Port
    );

    // IMiniportWaveRT
    STDMETHODIMP GetDeviceDescription(
        _Out_       PDEVICE_DESCRIPTION DeviceDescription
    );

    STDMETHODIMP NewStream(
        _Out_       PMINIPORTWAVERTSTREAM*  Stream,
        _In_opt_    PPORTWAVERTSTREAM       PortStream,
        _In_        ULONG                   Pin,
        _In_        BOOLEAN                 Capture,
        _In_        PKSDATAFORMAT           DataFormat
    );

    CIoctlTransport* GetTransport() const { return m_Transport; }
};

//=============================================================================
// CMiniportWaveRTStream
// Implements IMiniportWaveRTStreamNotification for glitch-free event-driven capture
//=============================================================================
class CMiniportWaveRTStream : public IMiniportWaveRTStreamNotification,
                              public CUnknown {
private:
    CMiniportWaveRT*    m_Miniport;
    CIoctlTransport*    m_Transport;
    BOOLEAN             m_Capture;
    AudioDataFormat     m_Format;
    KSSTATE             m_State;

    // Cyclic DMA Buffer
    PVOID               m_DmaBuffer;
    ULONG               m_DmaBufferSize;
    ULONG               m_WriteOffset;

    // Notification Timer & DPC
    KTIMER              m_NotificationTimer;
    KDPC                m_NotificationDpc;
    PRKEVENT            m_NotificationEvents[2];
    ULONG               m_NotificationEventsCount;

    // Clock and Position
    ULONGLONG           m_LinearPosition;
    LARGE_INTEGER       m_StartTime;

    static VOID TimerDpcRoutine(
        _In_     struct _KDPC *Dpc,
        _In_opt_ PVOID        DeferredContext,
        _In_opt_ PVOID        SystemArgument1,
        _In_opt_ PVOID        SystemArgument2
    );

    VOID ProcessAudioHop();

public:
    DECLARE_STD_UNKNOWN();

    CMiniportWaveRTStream(
        _In_opt_ PUNKNOWN UnknownOuter,
        _In_ CMiniportWaveRT* Miniport,
        _In_ BOOLEAN Capture,
        _In_ PKSDATAFORMAT DataFormat
    );
    virtual ~CMiniportWaveRTStream();

    NTSTATUS Initialize();

    // IMiniportWaveRTStream
    STDMETHODIMP SetState(
        _In_        KSSTATE State
    );

    STDMETHODIMP GetPosition(
        _Out_       PKSRTAUDIO_GETPOSITION_INFO PositionInfo
    );

    STDMETHODIMP AllocateAudioBuffer(
        _In_        ULONG                   RequestedSize,
        _Out_       PMDL*                   AudioBufferMdl,
        _Out_       ULONG*                  ActualSize,
        _Out_       ULONG*                  OffsetFromFirstPage,
        _Out_       MEMORY_CACHING_TYPE*    CacheType
    );

    STDMETHODIMP FreeAudioBuffer(
        _In_opt_    PMDL    AudioBufferMdl,
        _In_        ULONG   BufferSize
    );

    STDMETHODIMP GetClockRegister(
        _Out_       PKSRTAUDIO_HWREGISTER   Register
    );

    STDMETHODIMP GetPositionRegister(
        _Out_       PKSRTAUDIO_HWREGISTER   Register
    );

    STDMETHODIMP GetHardwareLatency(
        _Out_       PKSRTAUDIO_HWLATENCY    Latency
    );

    STDMETHODIMP SetFormat(
        _In_        PKSDATAFORMAT Format
    );

    // IMiniportWaveRTStreamNotification
    STDMETHODIMP RegisterNotificationEvent(
        _In_        PKEVENT NotificationEvent
    );

    STDMETHODIMP UnregisterNotificationEvent(
        _In_        PKEVENT NotificationEvent
    );
};

// Factory functions
NTSTATUS CreateMiniportWaveRT(
    _Outptr_    PUNKNOWN*        Unknown,
    _In_opt_    PUNKNOWN         UnknownOuter,
    _In_        POOL_FLAGS       PoolFlags,
    _In_        CIoctlTransport* Transport
);
