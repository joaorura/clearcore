#include "Driver.h"

// Device interface GUID definitions
static const GUID GUID_KSCATEGORY_AUDIO    = { 0x6994AD04, 0x93EF, 0x11D0, { 0xA3, 0xCC, 0x00, 0xA0, 0xC9, 0x22, 0x31, 0x96 } };
static const GUID GUID_KSCATEGORY_CAPTURE  = { 0x65E8773D, 0x8F56, 0x11D0, { 0xA3, 0xB9, 0x00, 0xA0, 0xC9, 0x22, 0x31, 0x96 } };
static const GUID GUID_KSCATEGORY_REALTIME = { 0xEB115FFC, 0x10C8, 0x4964, { 0x83, 0x1D, 0x6D, 0xCB, 0x02, 0xE6, 0xF2, 0x3F } };

// Forward declarations of adapter-level helpers
static NTSTATUS StartDevice(
    _In_ PDEVICE_OBJECT DeviceObject,
    _In_ PIRP           Irp,
    _In_ PRESOURCELIST  ResourceList
);

//=============================================================================
// Driver Entry Point
//=============================================================================
extern "C" NTSTATUS DriverEntry(
    _In_ PDRIVER_OBJECT   DriverObject,
    _In_ PUNICODE_STRING  RegistryPath
)
{
    NTSTATUS status;

    // Initialize PortCls adapter driver
    status = PcInitializeAdapterDriver(
        DriverObject,
        RegistryPath,
        (PDRIVER_ADD_DEVICE)DriverAddDevice
    );

    if (!NT_SUCCESS(status)) {
        return status;
    }

    // Set major function dispatch routines
    DriverObject->MajorFunction[IRP_MJ_CREATE]         = DriverDispatchCreate;
    DriverObject->MajorFunction[IRP_MJ_CLOSE]          = DriverDispatchClose;
    DriverObject->MajorFunction[IRP_MJ_CLEANUP]        = DriverDispatchCleanup;
    DriverObject->MajorFunction[IRP_MJ_DEVICE_CONTROL] = DriverDispatchDeviceControl;
    DriverObject->MajorFunction[IRP_MJ_POWER]          = DriverDispatchPower;
    DriverObject->MajorFunction[IRP_MJ_PNP]            = DriverDispatchPnp;
    DriverObject->DriverUnload                         = DriverUnload;

    return STATUS_SUCCESS;
}

//=============================================================================
// Driver Unload
//=============================================================================
extern "C" VOID DriverUnload(
    _In_ PDRIVER_OBJECT DriverObject
)
{
    UNREFERENCED_PARAMETER(DriverObject);
}

//=============================================================================
// AddDevice
//=============================================================================
extern "C" NTSTATUS DriverAddDevice(
    _In_ PDRIVER_OBJECT DriverObject,
    _In_ PDEVICE_OBJECT PhysicalDeviceObject
)
{
    PDEVICE_OBJECT deviceObject = nullptr;

    NTSTATUS status = PcAddAdapterDevice(
        DriverObject,
        PhysicalDeviceObject,
        &deviceObject,
        sizeof(DEVICE_EXTENSION),
        0
    );

    if (!NT_SUCCESS(status) || deviceObject == nullptr) {
        return status;
    }

    DEVICE_EXTENSION* devExt = (DEVICE_EXTENSION*)deviceObject->DeviceExtension;
    RtlZeroMemory(devExt, sizeof(DEVICE_EXTENSION));

    devExt->DeviceObject = deviceObject;
    devExt->PhysicalDeviceObject = PhysicalDeviceObject;

    // Allocate IOCTL transport coordinator
    devExt->Transport = new (NonPagedPoolNx, 'tNoN') CIoctlTransport();
    if (devExt->Transport == nullptr) {
        return STATUS_INSUFFICIENT_RESOURCES;
    }

    status = devExt->Transport->Initialize();
    if (!NT_SUCCESS(status)) {
        delete devExt->Transport;
        devExt->Transport = nullptr;
        return status;
    }

    // Initialize PortCls WaveRT Port and Miniport
    PPORTWAVERT port = nullptr;
    status = PcNewPort((PPORT*)&port, CLSID_PortWaveRT);
    if (!NT_SUCCESS(status)) {
        delete devExt->Transport;
        devExt->Transport = nullptr;
        return status;
    }

    PUNKNOWN miniportUnknown = nullptr;
    status = CreateMiniportWaveRT(
        &miniportUnknown,
        nullptr,
        POOL_FLAG_NON_PAGED,
        devExt->Transport
    );

    if (!NT_SUCCESS(status)) {
        port->Release();
        delete devExt->Transport;
        devExt->Transport = nullptr;
        return status;
    }

    status = port->Init(
        deviceObject,
        nullptr,
        miniportUnknown,
        nullptr
    );

    if (!NT_SUCCESS(status)) {
        miniportUnknown->Release();
        port->Release();
        delete devExt->Transport;
        devExt->Transport = nullptr;
        return status;
    }

    // Register subdevice under name "Wave"
    status = PcRegisterSubdevice(
        deviceObject,
        L"Wave",
        port
    );

    if (!NT_SUCCESS(status)) {
        miniportUnknown->Release();
        port->Release();
        delete devExt->Transport;
        devExt->Transport = nullptr;
        return status;
    }

    devExt->WavePort = port;
    devExt->WaveMiniport = miniportUnknown;

    // Register device interfaces for virtual audio capture
    PcRegisterAdapterPnpInterface(PhysicalDeviceObject, &GUID_KSCATEGORY_AUDIO);
    PcRegisterAdapterPnpInterface(PhysicalDeviceObject, &GUID_KSCATEGORY_CAPTURE);
    PcRegisterAdapterPnpInterface(PhysicalDeviceObject, &GUID_KSCATEGORY_REALTIME);

    deviceObject->Flags &= ~DO_DEVICE_INITIALIZING;

    return STATUS_SUCCESS;
}

//=============================================================================
// Dispatch Routines
//=============================================================================

extern "C" NTSTATUS DriverDispatchCreate(
    _In_ PDEVICE_OBJECT DeviceObject,
    _In_ PIRP           Irp
)
{
    return PcDispatchIrp(DeviceObject, Irp);
}

extern "C" NTSTATUS DriverDispatchClose(
    _In_ PDEVICE_OBJECT DeviceObject,
    _In_ PIRP           Irp
)
{
    return PcDispatchIrp(DeviceObject, Irp);
}

extern "C" NTSTATUS DriverDispatchCleanup(
    _In_ PDEVICE_OBJECT DeviceObject,
    _In_ PIRP           Irp
)
{
    DEVICE_EXTENSION* devExt = (DEVICE_EXTENSION*)DeviceObject->DeviceExtension;
    PIO_STACK_LOCATION irpSp = IoGetCurrentIrpStackLocation(Irp);

    if (devExt != nullptr && devExt->Transport != nullptr && irpSp != nullptr) {
        // Automatically release session ownership if caller handle closes/terminates
        devExt->Transport->HandleFileCleanup(irpSp->FileObject);
    }

    return PcDispatchIrp(DeviceObject, Irp);
}

extern "C" NTSTATUS DriverDispatchPower(
    _In_ PDEVICE_OBJECT DeviceObject,
    _In_ PIRP           Irp
)
{
    return PcDispatchIrp(DeviceObject, Irp);
}

extern "C" NTSTATUS DriverDispatchPnp(
    _In_ PDEVICE_OBJECT DeviceObject,
    _In_ PIRP           Irp
)
{
    return PcDispatchIrp(DeviceObject, Irp);
}

extern "C" NTSTATUS DriverDispatchDeviceControl(
    _In_ PDEVICE_OBJECT DeviceObject,
    _In_ PIRP           Irp
)
{
    PIO_STACK_LOCATION irpSp = IoGetCurrentIrpStackLocation(Irp);
    DEVICE_EXTENSION* devExt = (DEVICE_EXTENSION*)DeviceObject->DeviceExtension;

    if (devExt == nullptr || devExt->Transport == nullptr) {
        Irp->IoStatus.Status = STATUS_DEVICE_NOT_READY;
        Irp->IoStatus.Information = 0;
        IoCompleteRequest(Irp, IO_NO_INCREMENT);
        return STATUS_DEVICE_NOT_READY;
    }

    const ULONG ioctlCode = irpSp->Parameters.DeviceIoControl.IoControlCode;

    switch (ioctlCode) {
    case IOCTL_REALTIME_NOISE_SUBMIT_ENVELOPE:
        {
            const ULONG inputLength = irpSp->Parameters.DeviceIoControl.InputBufferLength;

            // Direct I/O: input buffer is described by MDL
            if (Irp->MdlAddress == nullptr) {
                Irp->IoStatus.Status = STATUS_INVALID_PARAMETER;
                Irp->IoStatus.Information = 0;
                IoCompleteRequest(Irp, IO_NO_INCREMENT);
                return STATUS_INVALID_PARAMETER;
            }

            PVOID systemBuffer = MmGetSystemAddressForMdlSafe(
                Irp->MdlAddress,
                NormalPagePriority | MdlMappingNoExecute
            );

            if (systemBuffer == nullptr) {
                Irp->IoStatus.Status = STATUS_INSUFFICIENT_RESOURCES;
                Irp->IoStatus.Information = 0;
                IoCompleteRequest(Irp, IO_NO_INCREMENT);
                return STATUS_INSUFFICIENT_RESOURCES;
            }

            ULONG sessionId = 0;
            IoGetRequestorSessionId(Irp, &sessionId);
            HANDLE processId = (HANDLE)IoGetRequestorProcessId(Irp);

            NTSTATUS status = devExt->Transport->SubmitEnvelope(
                irpSp->FileObject,
                sessionId,
                processId,
                (const WireFrameEnvelopeV1*)systemBuffer,
                inputLength
            );

            Irp->IoStatus.Status = status;
            Irp->IoStatus.Information = 0;
            IoCompleteRequest(Irp, IO_NO_INCREMENT);
            return status;
        }

    case IOCTL_REALTIME_NOISE_ACQUIRE_SESSION:
        {
            ULONG sessionId = 0;
            IoGetRequestorSessionId(Irp, &sessionId);
            HANDLE processId = (HANDLE)IoGetRequestorProcessId(Irp);

            NTSTATUS status = devExt->Transport->AcquireSession(
                irpSp->FileObject,
                sessionId,
                processId
            );

            Irp->IoStatus.Status = status;
            Irp->IoStatus.Information = 0;
            IoCompleteRequest(Irp, IO_NO_INCREMENT);
            return status;
        }

    case IOCTL_REALTIME_NOISE_RELEASE_SESSION:
        {
            NTSTATUS status = devExt->Transport->ReleaseSession(irpSp->FileObject);

            Irp->IoStatus.Status = status;
            Irp->IoStatus.Information = 0;
            IoCompleteRequest(Irp, IO_NO_INCREMENT);
            return status;
        }

    case IOCTL_REALTIME_NOISE_GET_STATS:
        {
            const ULONG outputLength = irpSp->Parameters.DeviceIoControl.OutputBufferLength;
            if (outputLength < sizeof(TransportStats)) {
                Irp->IoStatus.Status = STATUS_BUFFER_TOO_SMALL;
                Irp->IoStatus.Information = sizeof(TransportStats);
                IoCompleteRequest(Irp, IO_NO_INCREMENT);
                return STATUS_BUFFER_TOO_SMALL;
            }

            PVOID outputBuffer = Irp->AssociatedIrp.SystemBuffer;
            if (outputBuffer == nullptr) {
                Irp->IoStatus.Status = STATUS_INVALID_PARAMETER;
                Irp->IoStatus.Information = 0;
                IoCompleteRequest(Irp, IO_NO_INCREMENT);
                return STATUS_INVALID_PARAMETER;
            }

            devExt->Transport->GetStats((TransportStats*)outputBuffer);

            Irp->IoStatus.Status = STATUS_SUCCESS;
            Irp->IoStatus.Information = sizeof(TransportStats);
            IoCompleteRequest(Irp, IO_NO_INCREMENT);
            return STATUS_SUCCESS;
        }

    default:
        // Pass standard KS property / PortCls IOCTLs to PortCls
        return PcDispatchIrp(DeviceObject, Irp);
    }
}
