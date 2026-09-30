#pragma once

#include <portcls.h>
#include <ks.h>
#include <ksmedia.h>
#include "IoctlTransport.h"
#include "WaveRtMiniport.h"

#define DRIVER_TAG 'dNoN'

// Device Extension stored in PortCls adapter device
struct DEVICE_EXTENSION {
    PDEVICE_OBJECT      DeviceObject;
    PDEVICE_OBJECT      PhysicalDeviceObject;
    CIoctlTransport*    Transport;
    PPORTWAVERT         WavePort;
    PUNKNOWN            WaveMiniport;
};

//
// Driver Lifecycle & Dispatch Declarations
//

extern "C" {

DRIVER_INITIALIZE DriverEntry;
DRIVER_UNLOAD     DriverUnload;
DRIVER_ADD_DEVICE DriverAddDevice;

__drv_dispatchType(IRP_MJ_CREATE)
DRIVER_DISPATCH DriverDispatchCreate;

__drv_dispatchType(IRP_MJ_CLOSE)
DRIVER_DISPATCH DriverDispatchClose;

__drv_dispatchType(IRP_MJ_CLEANUP)
DRIVER_DISPATCH DriverDispatchCleanup;

__drv_dispatchType(IRP_MJ_DEVICE_CONTROL)
DRIVER_DISPATCH DriverDispatchDeviceControl;

__drv_dispatchType(IRP_MJ_POWER)
DRIVER_DISPATCH DriverDispatchPower;

__drv_dispatchType(IRP_MJ_PNP)
DRIVER_DISPATCH DriverDispatchPnp;

} // extern "C"
