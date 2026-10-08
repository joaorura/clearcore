#include "WaveRtMiniport.h"

//
// KS Data Ranges for 48 kHz Mono IEEE Float32 and PCM16
//
static const KSDATARANGE_AUDIO PinDataRangesAudio[] = {
    // 1. Mono 48 kHz 32-bit Float
    {
        {
            sizeof(KSDATARANGE_AUDIO),
            0,
            0,
            0,
            STATICGUIDOF(KSDATAFORMAT_TYPE_AUDIO),
            STATICGUIDOF(KSDATAFORMAT_SUBTYPE_IEEE_FLOAT),
            STATICGUIDOF(KSDATAFORMAT_SPECIFIER_WAVEFORMATEX)
        },
        WAVERT_CAPTURE_CHANNELS,
        32,
        32,
        WAVERT_CAPTURE_SAMPLE_RATE_HZ,
        WAVERT_CAPTURE_SAMPLE_RATE_HZ
    },
    // 2. Mono 48 kHz 16-bit PCM
    {
        {
            sizeof(KSDATARANGE_AUDIO),
            0,
            0,
            0,
            STATICGUIDOF(KSDATAFORMAT_TYPE_AUDIO),
            STATICGUIDOF(KSDATAFORMAT_SUBTYPE_PCM),
            STATICGUIDOF(KSDATAFORMAT_SPECIFIER_WAVEFORMATEX)
        },
        WAVERT_CAPTURE_CHANNELS,
        16,
        16,
        WAVERT_CAPTURE_SAMPLE_RATE_HZ,
        WAVERT_CAPTURE_SAMPLE_RATE_HZ
    }
};

static const PKSDATARANGE PinDataRangePointers[] = {
    (PKSDATARANGE)&PinDataRangesAudio[0],
    (PKSDATARANGE)&PinDataRangesAudio[1]
};

//
// Virtual Capture Pin Descriptor
//
static const PCPIN_DESCRIPTOR MiniportPins[] = {
    {
        1, 1, 0, // MaxDataFlows, MaxFilterInstanceCount, MinFilterInstanceCount
        NULL,    // AutomationTable
        {
            0,
            NULL,
            0,
            NULL,
            SIZEOF_ARRAY(PinDataRangePointers),
            PinDataRangePointers,
            KSPIN_DATAFLOW_OUT,
            KSPIN_COMMUNICATION_SINK,
            &PINNAME_CAPTURE,
            &KSCATEGORY_AUDIO,
            0
        }
    }
};

static const PCFILTER_DESCRIPTOR MiniportFilterDescriptor = {
    0,                                  // Version
    NULL,                               // AutomationTable
    sizeof(PCPIN_DESCRIPTOR),          // PinSize
    SIZEOF_ARRAY(MiniportPins),         // PinCount
    MiniportPins,                       // Pins
    0,                                  // NodeSize
    0,                                  // NodeCount
    NULL,                               // Nodes
    0,                                  // ConnectionCount
    NULL,                               // Connections
    0,                                  // CategoryCount
    NULL                                // Categories
};

//=============================================================================
// CMiniportWaveRT Implementation
//=============================================================================

CMiniportWaveRT::CMiniportWaveRT(
    _In_opt_ PUNKNOWN UnknownOuter,
    _In_ CIoctlTransport* Transport
) : CUnknown(UnknownOuter),
    m_PortStream(nullptr),
    m_DeviceObject(nullptr),
    m_Transport(Transport)
{
}

CMiniportWaveRT::~CMiniportWaveRT()
{
}

STDMETHODIMP_(NTSTATUS) CMiniportWaveRT::NonDelegatingQueryInterface(
    _In_ REFIID Interface,
    _Outptr_ PVOID* Object
)
{
    if (IsEqualGUIDAligned(Interface, IID_IUnknown)) {
        *Object = PVOID(PUNKNOWN(this));
    } else if (IsEqualGUIDAligned(Interface, IID_IMiniport)) {
        *Object = PVOID(PMINIPORT(this));
    } else if (IsEqualGUIDAligned(Interface, IID_IMiniportWaveRT)) {
        *Object = PVOID(PMINIPORTWAVERT(this));
    } else {
        *Object = nullptr;
        return STATUS_INVALID_PARAMETER;
    }

    ((PUNKNOWN)*Object)->AddRef();
    return STATUS_SUCCESS;
}

STDMETHODIMP CMiniportWaveRT::Init(
    _In_ PUNKNOWN UnknownAdapter,
    _In_ PRESOURCELIST ResourceList,
    _In_ PPORTWAVERT Port
)
{
    UNREFERENCED_PARAMETER(UnknownAdapter);
    UNREFERENCED_PARAMETER(ResourceList);
    UNREFERENCED_PARAMETER(Port);

    return STATUS_SUCCESS;
}

STDMETHODIMP CMiniportWaveRT::GetDescription(
    _Out_ PPCFILTER_DESCRIPTOR* Description
)
{
    if (Description == nullptr) {
        return STATUS_INVALID_PARAMETER;
    }
    *Description = (PPCFILTER_DESCRIPTOR)&MiniportFilterDescriptor;
    return STATUS_SUCCESS;
}

STDMETHODIMP CMiniportWaveRT::DataRangeIntersection(
    _In_ ULONG PinId,
    _In_ PKSDATARANGE ClientDataRange,
    _In_ PKSDATARANGE MyDataRange,
    _In_ ULONG OutputBufferLength,
    _Out_writes_bytes_to_opt_(OutputBufferLength, *ResultLength) PVOID ResultData,
    _Out_ PULONG ResultLength
)
{
    UNREFERENCED_PARAMETER(PinId);
    UNREFERENCED_PARAMETER(ClientDataRange);
    UNREFERENCED_PARAMETER(MyDataRange);
    UNREFERENCED_PARAMETER(OutputBufferLength);
    UNREFERENCED_PARAMETER(ResultData);
    UNREFERENCED_PARAMETER(ResultLength);

    return STATUS_NOT_IMPLEMENTED;
}

STDMETHODIMP CMiniportWaveRT::GetDeviceDescription(
    _Out_ PDEVICE_DESCRIPTION DeviceDescription
)
{
    if (DeviceDescription == nullptr) {
        return STATUS_INVALID_PARAMETER;
    }

    RtlZeroMemory(DeviceDescription, sizeof(DEVICE_DESCRIPTION));
    DeviceDescription->Version = DEVICE_DESCRIPTION_VERSION;
    DeviceDescription->Master = TRUE;
    DeviceDescription->ScatterGather = TRUE;
    DeviceDescription->Dma32BitAddresses = TRUE;
    DeviceDescription->InterfaceType = Internal;
    DeviceDescription->MaximumLength = 0xFFFFFFFF;

    return STATUS_SUCCESS;
}

STDMETHODIMP CMiniportWaveRT::NewStream(
    _Out_ PMINIPORTWAVERTSTREAM* Stream,
    _In_opt_ PPORTWAVERTSTREAM PortStream,
    _In_ ULONG Pin,
    _In_ BOOLEAN Capture,
    _In_ PKSDATAFORMAT DataFormat
)
{
    UNREFERENCED_PARAMETER(PortStream);

    if (Stream == nullptr || DataFormat == nullptr) {
        return STATUS_INVALID_PARAMETER;
    }

    if (Pin != 0 || !Capture) {
        return STATUS_INVALID_PARAMETER;
    }

    // Verify format: mono 48 kHz Float32 or PCM16
    if (!IsEqualGUIDAligned(DataFormat->MajorFormat, KSDATAFORMAT_TYPE_AUDIO) ||
        !IsEqualGUIDAligned(DataFormat->Specifier, KSDATAFORMAT_SPECIFIER_WAVEFORMATEX)) {
        return STATUS_NO_MATCH;
    }

    PWAVEFORMATEX waveFormat = (PWAVEFORMATEX)(DataFormat + 1);
    if (waveFormat->nSamplesPerSec != WAVERT_CAPTURE_SAMPLE_RATE_HZ ||
        waveFormat->nChannels != WAVERT_CAPTURE_CHANNELS) {
        return STATUS_NO_MATCH;
    }

    if (waveFormat->wBitsPerSample != 32 && waveFormat->wBitsPerSample != 16) {
        return STATUS_NO_MATCH;
    }

    CMiniportWaveRTStream* stream = new (NonPagedPoolNx, 'eNoN') CMiniportWaveRTStream(
        nullptr,
        this,
        Capture,
        DataFormat
    );

    if (stream == nullptr) {
        return STATUS_INSUFFICIENT_RESOURCES;
    }

    NTSTATUS status = stream->Initialize();
    if (!NT_SUCCESS(status)) {
        delete stream;
        return status;
    }

    *Stream = PMINIPORTWAVERTSTREAM(stream);
    (*Stream)->AddRef();

    return STATUS_SUCCESS;
}

//=============================================================================
// CMiniportWaveRTStream Implementation
//=============================================================================

CMiniportWaveRTStream::CMiniportWaveRTStream(
    _In_opt_ PUNKNOWN UnknownOuter,
    _In_ CMiniportWaveRT* Miniport,
    _In_ BOOLEAN Capture,
    _In_ PKSDATAFORMAT DataFormat
) : CUnknown(UnknownOuter),
    m_Miniport(Miniport),
    m_Transport(Miniport->GetTransport()),
    m_Capture(Capture),
    m_State(KSSTATE_STOP),
    m_DmaBuffer(nullptr),
    m_DmaBufferSize(0),
    m_WriteOffset(0),
    m_NotificationEventsCount(0),
    m_LinearPosition(0)
{
    PWAVEFORMATEX waveFormat = (PWAVEFORMATEX)(DataFormat + 1);
    if (waveFormat->wBitsPerSample == 32) {
        m_Format = AudioFormat_Float32;
    } else {
        m_Format = AudioFormat_Pcm16;
    }

    m_NotificationEvents[0] = nullptr;
    m_NotificationEvents[1] = nullptr;
    m_StartTime.QuadPart = 0;
}

CMiniportWaveRTStream::~CMiniportWaveRTStream()
{
    if (m_State == KSSTATE_RUN) {
        SetState(KSSTATE_STOP);
    }
}

STDMETHODIMP_(NTSTATUS) CMiniportWaveRTStream::NonDelegatingQueryInterface(
    _In_ REFIID Interface,
    _Outptr_ PVOID* Object
)
{
    if (IsEqualGUIDAligned(Interface, IID_IUnknown)) {
        *Object = PVOID(PUNKNOWN(this));
    } else if (IsEqualGUIDAligned(Interface, IID_IMiniportWaveRTStream)) {
        *Object = PVOID(PMINIPORTWAVERTSTREAM(this));
    } else if (IsEqualGUIDAligned(Interface, IID_IMiniportWaveRTStreamNotification)) {
        *Object = PVOID(PMINIPORTWAVERTSTREAMNOTIFICATION(this));
    } else {
        *Object = nullptr;
        return STATUS_INVALID_PARAMETER;
    }

    ((PUNKNOWN)*Object)->AddRef();
    return STATUS_SUCCESS;
}

NTSTATUS CMiniportWaveRTStream::Initialize()
{
    KeInitializeTimer(&m_NotificationTimer);
    KeInitializeDpc(&m_NotificationDpc, TimerDpcRoutine, this);
    return STATUS_SUCCESS;
}

VOID CMiniportWaveRTStream::TimerDpcRoutine(
    _In_ struct _KDPC *Dpc,
    _In_opt_ PVOID DeferredContext,
    _In_opt_ PVOID SystemArgument1,
    _In_opt_ PVOID SystemArgument2
)
{
    UNREFERENCED_PARAMETER(Dpc);
    UNREFERENCED_PARAMETER(SystemArgument1);
    UNREFERENCED_PARAMETER(SystemArgument2);

    CMiniportWaveRTStream* stream = (CMiniportWaveRTStream*)DeferredContext;
    if (stream != nullptr) {
        stream->ProcessAudioHop();
    }
}

VOID CMiniportWaveRTStream::ProcessAudioHop()
{
    if (m_State != KSSTATE_RUN || m_DmaBuffer == nullptr || m_DmaBufferSize == 0) {
        return;
    }

    FLOAT tempFloatBuffer[WIRE_ENVELOPE_HOP_SAMPLES];
    const UINT32 bytesPerSample = (m_Format == AudioFormat_Float32) ? 4 : 2;
    const ULONG hopBytes = WIRE_ENVELOPE_HOP_SAMPLES * bytesPerSample;

    // Zero-raw-leakage fail-closed policy:
    // If transport has denoised samples, consume them.
    // If engine is not running or underrun occurs, output pure digital silence.
    BOOLEAN hasDenoisedSamples = FALSE;
    if (m_Transport != nullptr) {
        hasDenoisedSamples = m_Transport->ConsumeSamples(tempFloatBuffer, WIRE_ENVELOPE_HOP_SAMPLES);
    }

    PUCHAR destPtr = (PUCHAR)m_DmaBuffer + m_WriteOffset;

    if (hasDenoisedSamples) {
        if (m_Format == AudioFormat_Float32) {
            RtlCopyMemory(destPtr, tempFloatBuffer, hopBytes);
        } else {
            // Convert Float32 [-1.0f, 1.0f] to PCM16
            SHORT* pcmDest = (SHORT*)destPtr;
            for (UINT32 i = 0; i < WIRE_ENVELOPE_HOP_SAMPLES; ++i) {
                FLOAT s = tempFloatBuffer[i];
                if (s >= 0.0f) {
                    if (s > 1.0f) s = 1.0f;
                    pcmDest[i] = (SHORT)(s * 32767.0f);
                } else if (s < 0.0f) {
                    if (s < -1.0f) s = -1.0f;
                    pcmDest[i] = (SHORT)(s * 32768.0f);
                } else {
                    // Non-finite (NaN/Inf) fails closed to pure digital silence
                    pcmDest[i] = 0;
                }
            }
        }
    } else {
        // Enforce strict fail-closed digital silence
        RtlZeroMemory(destPtr, hopBytes);
    }

    // Advance cyclic DMA write offset and monotonic linear position
    m_WriteOffset = (m_WriteOffset + hopBytes) % m_DmaBufferSize;
    m_LinearPosition += hopBytes;

    // Signal registered notification events to notify PortCls / WASAPI client
    for (ULONG i = 0; i < m_NotificationEventsCount; ++i) {
        if (m_NotificationEvents[i] != nullptr) {
            KeSetEvent(m_NotificationEvents[i], 0, FALSE);
        }
    }
}

STDMETHODIMP CMiniportWaveRTStream::SetState(
    _In_ KSSTATE State
)
{
    if (m_State == State) {
        return STATUS_SUCCESS;
    }

    KSSTATE oldState = m_State;

    switch (State) {
    case KSSTATE_STOP:
        KeCancelTimer(&m_NotificationTimer);
        m_WriteOffset = 0;
        m_LinearPosition = 0;
        if (m_DmaBuffer != nullptr && m_DmaBufferSize > 0) {
            RtlZeroMemory(m_DmaBuffer, m_DmaBufferSize);
        }
        if (oldState == KSSTATE_RUN && m_Transport != nullptr) {
            m_Transport->DecrementActiveStreams();
        }
        break;

    case KSSTATE_ACQUIRE:
        KeCancelTimer(&m_NotificationTimer);
        if (m_DmaBuffer != nullptr && m_DmaBufferSize > 0) {
            RtlZeroMemory(m_DmaBuffer, m_DmaBufferSize);
        }
        if (oldState == KSSTATE_RUN && m_Transport != nullptr) {
            m_Transport->DecrementActiveStreams();
        }
        break;

    case KSSTATE_PAUSE:
        KeCancelTimer(&m_NotificationTimer);
        if (m_DmaBuffer != nullptr && m_DmaBufferSize > 0) {
            RtlZeroMemory(m_DmaBuffer, m_DmaBufferSize);
        }
        if (oldState == KSSTATE_RUN && m_Transport != nullptr) {
            m_Transport->DecrementActiveStreams();
        }
        break;

    case KSSTATE_RUN:
        {
            if (m_DmaBuffer != nullptr && m_DmaBufferSize > 0) {
                RtlZeroMemory(m_DmaBuffer, m_DmaBufferSize);
            }
            m_WriteOffset = 0;
            m_LinearPosition = 0;

            // Start periodic timer for 10ms hops (100,000 * 100ns = 10ms)
            LARGE_INTEGER dueTime;
            dueTime.QuadPart = -100000LL; // 10ms relative
            KeSetTimerEx(&m_NotificationTimer, dueTime, 10, &m_NotificationDpc);

            if (oldState != KSSTATE_RUN && m_Transport != nullptr) {
                m_Transport->IncrementActiveStreams();
            }
        }
        break;
    }

    m_State = State;
    return STATUS_SUCCESS;
}

STDMETHODIMP CMiniportWaveRTStream::GetPosition(
    _Out_ PKSRTAUDIO_GETPOSITION_INFO PositionInfo
)
{
    if (PositionInfo == nullptr) {
        return STATUS_INVALID_PARAMETER;
    }

    PositionInfo->ActualPosition = m_WriteOffset;
    PositionInfo->LinearPosition = m_LinearPosition;
    KeQueryPerformanceCounter(&PositionInfo->PerformanceCounterPosition);

    return STATUS_SUCCESS;
}

STDMETHODIMP CMiniportWaveRTStream::AllocateAudioBuffer(
    _In_ ULONG RequestedSize,
    _Out_ PMDL* AudioBufferMdl,
    _Out_ ULONG* ActualSize,
    _Out_ ULONG* OffsetFromFirstPage,
    _Out_ MEMORY_CACHING_TYPE* CacheType
)
{
    if (AudioBufferMdl == nullptr || ActualSize == nullptr ||
        OffsetFromFirstPage == nullptr || CacheType == nullptr) {
        return STATUS_INVALID_PARAMETER;
    }

    const UINT32 bytesPerSample = (m_Format == AudioFormat_Float32) ? 4 : 2;
    const ULONG minBufferSize = WIRE_ENVELOPE_HOP_SAMPLES * bytesPerSample * WAVERT_CAPTURE_BUFFER_HOPS;

    ULONG bufferSize = (RequestedSize < minBufferSize) ? minBufferSize : RequestedSize;
    // Align buffer size to page size
    bufferSize = (ULONG)ROUND_TO_PAGES(bufferSize);

    PVOID buffer = ExAllocatePool2(POOL_FLAG_NON_PAGED, bufferSize, 'bNoN');
    if (buffer == nullptr) {
        return STATUS_INSUFFICIENT_RESOURCES;
    }

    // Fail-closed: zero memory immediately
    RtlZeroMemory(buffer, bufferSize);

    PMDL mdl = IoAllocateMdl(buffer, bufferSize, FALSE, FALSE, nullptr);
    if (mdl == nullptr) {
        ExFreePoolWithTag(buffer, 'bNoN');
        return STATUS_INSUFFICIENT_RESOURCES;
    }

    MmBuildMdlForNonPagedPool(mdl);

    m_DmaBuffer = buffer;
    m_DmaBufferSize = bufferSize;
    m_WriteOffset = 0;
    m_LinearPosition = 0;

    *AudioBufferMdl = mdl;
    *ActualSize = bufferSize;
    *OffsetFromFirstPage = 0;
    *CacheType = MmCached;

    return STATUS_SUCCESS;
}

STDMETHODIMP CMiniportWaveRTStream::FreeAudioBuffer(
    _In_opt_ PMDL AudioBufferMdl,
    _In_ ULONG BufferSize
)
{
    UNREFERENCED_PARAMETER(BufferSize);

    if (m_DmaBuffer != nullptr) {
        // Enforce zero-leakage on stream cleanup
        RtlZeroMemory(m_DmaBuffer, m_DmaBufferSize);
        ExFreePoolWithTag(m_DmaBuffer, 'bNoN');
        m_DmaBuffer = nullptr;
        m_DmaBufferSize = 0;
    }

    if (AudioBufferMdl != nullptr) {
        IoFreeMdl(AudioBufferMdl);
    }

    return STATUS_SUCCESS;
}

STDMETHODIMP CMiniportWaveRTStream::GetClockRegister(
    _Out_ PKSRTAUDIO_HWREGISTER Register
)
{
    UNREFERENCED_PARAMETER(Register);
    return STATUS_NOT_IMPLEMENTED;
}

STDMETHODIMP CMiniportWaveRTStream::GetPositionRegister(
    _Out_ PKSRTAUDIO_HWREGISTER Register
)
{
    UNREFERENCED_PARAMETER(Register);
    return STATUS_NOT_IMPLEMENTED;
}

STDMETHODIMP CMiniportWaveRTStream::GetHardwareLatency(
    _Out_ PKSRTAUDIO_HWLATENCY Latency
)
{
    if (Latency == nullptr) {
        return STATUS_INVALID_PARAMETER;
    }

    // 10ms hop latency
    Latency->FifoSize = WIRE_ENVELOPE_HOP_SAMPLES * ((m_Format == AudioFormat_Float32) ? 4 : 2);
    Latency->ChipsetDelay = 0;
    Latency->CodecDelay = 0;

    return STATUS_SUCCESS;
}

STDMETHODIMP CMiniportWaveRTStream::SetFormat(
    _In_ PKSDATAFORMAT Format
)
{
    UNREFERENCED_PARAMETER(Format);
    return STATUS_SUCCESS;
}

STDMETHODIMP CMiniportWaveRTStream::RegisterNotificationEvent(
    _In_ PKEVENT NotificationEvent
)
{
    if (NotificationEvent == nullptr) {
        return STATUS_INVALID_PARAMETER;
    }

    if (m_NotificationEventsCount >= SIZEOF_ARRAY(m_NotificationEvents)) {
        return STATUS_INSUFFICIENT_RESOURCES;
    }

    m_NotificationEvents[m_NotificationEventsCount++] = NotificationEvent;
    return STATUS_SUCCESS;
}

STDMETHODIMP CMiniportWaveRTStream::UnregisterNotificationEvent(
    _In_ PKEVENT NotificationEvent
)
{
    if (NotificationEvent == nullptr) {
        return STATUS_INVALID_PARAMETER;
    }

    for (ULONG i = 0; i < m_NotificationEventsCount; ++i) {
        if (m_NotificationEvents[i] == NotificationEvent) {
            for (ULONG j = i; j < m_NotificationEventsCount - 1; ++j) {
                m_NotificationEvents[j] = m_NotificationEvents[j + 1];
            }
            m_NotificationEventsCount--;
            m_NotificationEvents[m_NotificationEventsCount] = nullptr;
            return STATUS_SUCCESS;
        }
    }

    return STATUS_NOT_FOUND;
}

NTSTATUS CreateMiniportWaveRT(
    _Outptr_ PUNKNOWN* Unknown,
    _In_opt_ PUNKNOWN UnknownOuter,
    _In_ POOL_FLAGS PoolFlags,
    _In_ CIoctlTransport* Transport
)
{
    UNREFERENCED_PARAMETER(PoolFlags);

    if (Unknown == nullptr || Transport == nullptr) {
        return STATUS_INVALID_PARAMETER;
    }

    CMiniportWaveRT* miniport = new (NonPagedPoolNx, 'mNoN') CMiniportWaveRT(UnknownOuter, Transport);
    if (miniport == nullptr) {
        return STATUS_INSUFFICIENT_RESOURCES;
    }

    *Unknown = PUNKNOWN(miniport);
    (*Unknown)->AddRef();

    return STATUS_SUCCESS;
}
