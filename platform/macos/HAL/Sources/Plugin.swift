//===----------------------------------------------------------------------===//
//
// Plugin.swift
// RealtimeNoiseHAL
//
// CoreAudio AudioServerPlugInDriverInterface entry point for Clearcore Realtime Noise HAL.
// Features:
// - Registers manufacturer "CLRC", subtype "rtns".
// - Exposes Dual-Endpoint Topology:
//     * Visible Virtual Microphone (48kHz Float32 mono, selectable by applications).
//     * Hidden Loopback Output (non-mixable private stream for noise engine writer).
// - Zero memory allocation, zero blocking locks, zero synchronous RPCs in callbacks.
// - Enforces owner session lock; conflicting non-owner sessions receive `UnavailableBusy`.
// - Fail-closed: outputs digital silence (zeros) on underrun or absence.
// - Zero raw audio leakage: hardware mic audio is never bridged to visible input.
//
//===----------------------------------------------------------------------===//

import Foundation
import CoreAudio

#if canImport(Darwin)
import Darwin
import mach.mach_time
#endif

// MARK: - Constants & Identifiers

public let kManufacturerCLRC: String = "CLRC"
public let kSubtypeRTNS: String = "rtns"
public let kPluginBundleID: String = "com.clearcore.RealtimeNoiseHAL"
public let kPluginName: String = "Clearcore Realtime Noise HAL Plug-in"

// Standard CoreAudio HAL Plugin UUIDs
private let kAudioServerPlugInTypeUUID = CFUUIDGetUUIDBytes(
    CFUUIDCreateFromString(kCFAllocatorDefault, "443FD8E7-60B1-11D5-BCAC-0030654C991C" as CFString)
)
private let kAudioServerPlugInDriverInterfaceUUID = CFUUIDGetUUIDBytes(
    CFUUIDCreateFromString(kCFAllocatorDefault, "EEA5773D-CC43-49F1-8E00-8F96E7D23B17" as CFString)
)

// MARK: - Driver Shared State

public final class RealtimeNoiseDriver {
    public static let shared = RealtimeNoiseDriver()

    public let ringBuffer = RingBuffer(capacity: 16384)
    public let visibleInput = VisibleInputEndpoint()
    public let hiddenOutput = HiddenOutputEndpoint()

    private var hostRef: AudioServerPlugInHostRef?
    private var refCount: UInt32 = 1

    private var sampleTime: Float64 = 0.0
    private var hostTicksPerFrame: Float64 = 0.0

    private init() {
        var timebaseInfo = mach_timebase_info()
        mach_timebase_info(&timebaseInfo)
        let nanosPerSecond: Float64 = 1_000_000_000.0
        let ticksPerSecond = (nanosPerSecond * Float64(timebaseInfo.denom)) / Float64(timebaseInfo.numer)
        self.hostTicksPerFrame = ticksPerSecond / 48000.0
    }

    public func setHost(_ host: AudioServerPlugInHostRef) {
        self.hostRef = host
    }

    public func retain() -> UInt32 {
        refCount += 1
        return refCount
    }

    public func release() -> UInt32 {
        refCount = max(0, refCount - 1)
        return refCount
    }
}

// MARK: - C Interface Table Pointers

private var gDriverInterface = AudioServerPlugInDriverInterface(
    _reserved: nil,
    QueryInterface: Driver_QueryInterface,
    AddRef: Driver_AddRef,
    Release: Driver_Release,
    Initialize: Driver_Initialize,
    CreateDevice: Driver_CreateDevice,
    DestroyDevice: Driver_DestroyDevice,
    AddDeviceClient: Driver_AddDeviceClient,
    RemoveDeviceClient: Driver_RemoveDeviceClient,
    PerformDeviceConfigurationChange: Driver_PerformDeviceConfigurationChange,
    AbortDeviceConfigurationChange: Driver_AbortDeviceConfigurationChange,
    HasProperty: Driver_HasProperty,
    IsPropertySettable: Driver_IsPropertySettable,
    GetPropertyDataSize: Driver_GetPropertyDataSize,
    GetPropertyData: Driver_GetPropertyData,
    SetPropertyData: Driver_SetPropertyData,
    StartIO: Driver_StartIO,
    StopIO: Driver_StopIO,
    GetZeroTimeStamp: Driver_GetZeroTimeStamp,
    WillDoIOOperation: Driver_WillDoIOOperation,
    BeginIOOperation: Driver_BeginIOOperation,
    DoIOOperation: Driver_DoIOOperation,
    EndIOOperation: Driver_EndIOOperation
)

private var gDriverInterfacePtr: UnsafeMutablePointer<AudioServerPlugInDriverInterface>? = {
    let ptr = UnsafeMutablePointer<AudioServerPlugInDriverInterface>.allocate(capacity: 1)
    ptr.initialize(to: gDriverInterface)
    return ptr
}()

private var gDriverRef: AudioServerPlugInDriverRef? = {
    guard let interfacePtr = gDriverInterfacePtr else { return nil }
    let ptr = UnsafeMutablePointer<UnsafeMutablePointer<AudioServerPlugInDriverInterface>?>.allocate(capacity: 1)
    ptr.initialize(to: interfacePtr)
    return AudioServerPlugInDriverRef(ptr)
}()

// MARK: - Factory Function Entry Point

@_cdecl("RealtimeNoiseDriverFactory")
public func RealtimeNoiseDriverFactory(
    allocator: CFAllocator?,
    typeUUID: CFUUID?
) -> UnsafeMutableRawPointer? {
    guard let typeUUID = typeUUID else { return nil }
    let bytes = CFUUIDGetUUIDBytes(typeUUID)
    if memcmp(&kAudioServerPlugInTypeUUID, [bytes], MemoryLayout<CFUUIDBytes>.size) != 0 {
        return nil
    }

    _ = RealtimeNoiseDriver.shared.retain()
    return UnsafeMutableRawPointer(gDriverRef)
}

// MARK: - AudioServerPlugIn Driver Interface Implementation

private func Driver_QueryInterface(
    inDriver: UnsafeMutableRawPointer?,
    inUUID: REFIID,
    outInterface: UnsafeMutablePointer<LPVOID?>?
) -> HRESULT {
    guard let outInterface = outInterface else { return HRESULT(kAudioHardwareIllegalOperationError) }

    var reqBytes = CFUUIDBytes()
    memcpy(&reqBytes, inUUID, MemoryLayout<CFUUIDBytes>.size)

    if memcmp(&reqBytes, &kAudioServerPlugInDriverInterfaceUUID, MemoryLayout<CFUUIDBytes>.size) == 0 ||
       memcmp(&reqBytes, &kAudioServerPlugInTypeUUID, MemoryLayout<CFUUIDBytes>.size) == 0 {
        _ = RealtimeNoiseDriver.shared.retain()
        outInterface.pointee = UnsafeMutableRawPointer(gDriverRef)
        return 0 // S_OK
    }

    outInterface.pointee = nil
    return HRESULT(0x80004002) // E_NOINTERFACE
}

private func Driver_AddRef(inDriver: UnsafeMutableRawPointer?) -> ULONG {
    return ULONG(RealtimeNoiseDriver.shared.retain())
}

private func Driver_Release(inDriver: UnsafeMutableRawPointer?) -> ULONG {
    return ULONG(RealtimeNoiseDriver.shared.release())
}

private func Driver_Initialize(
    inDriver: AudioServerPlugInDriverRef?,
    inHost: AudioServerPlugInHostRef?
) -> OSStatus {
    guard let inHost = inHost else { return kAudioHardwareIllegalOperationError }
    RealtimeNoiseDriver.shared.setHost(inHost)
    return noErr
}

private func Driver_CreateDevice(
    inDriver: AudioServerPlugInDriverRef?,
    inDescription: CFDictionary?,
    inClientInfo: UnsafePointer<AudioServerPlugInClientInfo>?,
    outDeviceObjectID: UnsafeMutablePointer<AudioObjectID>?
) -> OSStatus {
    return kAudioHardwareUnsupportedOperationError
}

private func Driver_DestroyDevice(
    inDriver: AudioServerPlugInDriverRef?,
    inDeviceObjectID: AudioObjectID
) -> OSStatus {
    return kAudioHardwareUnsupportedOperationError
}

private func Driver_AddDeviceClient(
    inDriver: AudioServerPlugInDriverRef?,
    inDeviceObjectID: AudioObjectID,
    inClientInfo: UnsafePointer<AudioServerPlugInClientInfo>?
) -> OSStatus {
    guard let client = inClientInfo else { return kAudioHardwareIllegalOperationError }

    if inDeviceObjectID == HiddenOutputEndpoint.deviceObjectID {
        // Enforce owner session lock on hidden loopback endpoint
        let pid = client.pointee.mProcessID
        return RealtimeNoiseDriver.shared.hiddenOutput.acquireOwnerLock(clientPID: pid)
    }

    return noErr
}

private func Driver_RemoveDeviceClient(
    inDriver: AudioServerPlugInDriverRef?,
    inDeviceObjectID: AudioObjectID,
    inClientInfo: UnsafePointer<AudioServerPlugInClientInfo>?
) -> OSStatus {
    guard let client = inClientInfo else { return kAudioHardwareIllegalOperationError }

    if inDeviceObjectID == HiddenOutputEndpoint.deviceObjectID {
        let pid = client.pointee.mProcessID
        return RealtimeNoiseDriver.shared.hiddenOutput.releaseOwnerLock(clientPID: pid)
    }

    return noErr
}

private func Driver_PerformDeviceConfigurationChange(
    inDriver: AudioServerPlugInDriverRef?,
    inDeviceObjectID: AudioObjectID,
    inChangeAction: UInt64,
    inChangeInfo: UnsafeMutableRawPointer?
) -> OSStatus {
    return noErr
}

private func Driver_AbortDeviceConfigurationChange(
    inDriver: AudioServerPlugInDriverRef?,
    inDeviceObjectID: AudioObjectID,
    inChangeAction: UInt64,
    inChangeInfo: UnsafeMutableRawPointer?
) -> OSStatus {
    return noErr
}

private func Driver_HasProperty(
    inDriver: AudioServerPlugInDriverRef?,
    inObjectID: AudioObjectID,
    inClientProcessID: pid_t,
    inAddress: UnsafePointer<AudioObjectPropertyAddress>?
) -> DarwinBoolean {
    guard let addr = inAddress?.pointee else { return DarwinBoolean(false) }

    switch inObjectID {
    case kAudioObjectPlugInObject:
        switch addr.mSelector {
        case kAudioObjectPropertyBaseClass,
             kAudioObjectPropertyClass,
             kAudioObjectPropertyName,
             kAudioObjectPropertyManufacturer,
             kAudioPlugInPropertyBundleID,
             kAudioPlugInPropertyDeviceList,
             kAudioPlugInPropertyTranslateUIDToDevice:
            return DarwinBoolean(true)
        default:
            return DarwinBoolean(false)
        }
    case VisibleInputEndpoint.deviceObjectID:
        return DarwinBoolean(RealtimeNoiseDriver.shared.visibleInput.hasProperty(address: addr))
    case HiddenOutputEndpoint.deviceObjectID:
        return DarwinBoolean(RealtimeNoiseDriver.shared.hiddenOutput.hasProperty(address: addr))
    default:
        return DarwinBoolean(false)
    }
}

private func Driver_IsPropertySettable(
    inDriver: AudioServerPlugInDriverRef?,
    inObjectID: AudioObjectID,
    inClientProcessID: pid_t,
    inAddress: UnsafePointer<AudioObjectPropertyAddress>?,
    outIsSettable: UnsafeMutablePointer<DarwinBoolean>?
) -> OSStatus {
    guard let addr = inAddress?.pointee, let outSettable = outIsSettable else {
        return kAudioHardwareIllegalOperationError
    }

    switch inObjectID {
    case kAudioObjectPlugInObject:
        outSettable.pointee = DarwinBoolean(false)
        return noErr
    case VisibleInputEndpoint.deviceObjectID:
        outSettable.pointee = DarwinBoolean(RealtimeNoiseDriver.shared.visibleInput.isPropertySettable(address: addr))
        return noErr
    case HiddenOutputEndpoint.deviceObjectID:
        outSettable.pointee = DarwinBoolean(RealtimeNoiseDriver.shared.hiddenOutput.isPropertySettable(address: addr))
        return noErr
    default:
        outSettable.pointee = DarwinBoolean(false)
        return kAudioHardwareBadObjectError
    }
}

private func Driver_GetPropertyDataSize(
    inDriver: AudioServerPlugInDriverRef?,
    inObjectID: AudioObjectID,
    inClientProcessID: pid_t,
    inAddress: UnsafePointer<AudioObjectPropertyAddress>?,
    inQualifierDataSize: UInt32,
    inQualifierData: UnsafeRawPointer?,
    outDataSize: UnsafeMutablePointer<UInt32>?
) -> OSStatus {
    guard let addr = inAddress?.pointee, let outSize = outDataSize else {
        return kAudioHardwareIllegalOperationError
    }

    switch inObjectID {
    case kAudioObjectPlugInObject:
        switch addr.mSelector {
        case kAudioObjectPropertyBaseClass, kAudioObjectPropertyClass:
            outSize.pointee = UInt32(MemoryLayout<AudioClassID>.size)
        case kAudioObjectPropertyName, kAudioObjectPropertyManufacturer, kAudioPlugInPropertyBundleID:
            outSize.pointee = UInt32(MemoryLayout<CFStringRef>.size)
        case kAudioPlugInPropertyDeviceList:
            // Two dual-endpoint devices: Visible Input & Hidden Output
            outSize.pointee = UInt32(MemoryLayout<AudioObjectID>.size * 2)
        case kAudioPlugInPropertyTranslateUIDToDevice:
            outSize.pointee = UInt32(MemoryLayout<AudioObjectID>.size)
        default:
            return kAudioHardwareUnknownPropertyError
        }
        return noErr
    case VisibleInputEndpoint.deviceObjectID:
        outSize.pointee = RealtimeNoiseDriver.shared.visibleInput.getPropertyDataSize(address: addr)
        return outSize.pointee > 0 ? noErr : kAudioHardwareUnknownPropertyError
    case HiddenOutputEndpoint.deviceObjectID:
        outSize.pointee = RealtimeNoiseDriver.shared.hiddenOutput.getPropertyDataSize(address: addr)
        return outSize.pointee > 0 ? noErr : kAudioHardwareUnknownPropertyError
    default:
        return kAudioHardwareBadObjectError
    }
}

private func Driver_GetPropertyData(
    inDriver: AudioServerPlugInDriverRef?,
    inObjectID: AudioObjectID,
    inClientProcessID: pid_t,
    inAddress: UnsafePointer<AudioObjectPropertyAddress>?,
    inQualifierDataSize: UInt32,
    inQualifierData: UnsafeRawPointer?,
    inDataSize: UInt32,
    outDataSize: UnsafeMutablePointer<UInt32>?,
    outData: UnsafeMutableRawPointer?
) -> OSStatus {
    guard let addr = inAddress?.pointee, let outSize = outDataSize, let outPtr = outData else {
        return kAudioHardwareIllegalOperationError
    }

    switch inObjectID {
    case kAudioObjectPlugInObject:
        switch addr.mSelector {
        case kAudioObjectPropertyBaseClass:
            outPtr.assumingMemoryBound(to: AudioClassID.self).pointee = kAudioObjectClassID
            outSize.pointee = UInt32(MemoryLayout<AudioClassID>.size)
        case kAudioObjectPropertyClass:
            outPtr.assumingMemoryBound(to: AudioClassID.self).pointee = kAudioPlugInClassID
            outSize.pointee = UInt32(MemoryLayout<AudioClassID>.size)
        case kAudioObjectPropertyName:
            let str = kPluginName as CFString
            outPtr.assumingMemoryBound(to: CFStringRef.self).pointee = Unmanaged.passRetained(str).takeRetainedValue()
            outSize.pointee = UInt32(MemoryLayout<CFStringRef>.size)
        case kAudioObjectPropertyManufacturer:
            let str = kManufacturerCLRC as CFString
            outPtr.assumingMemoryBound(to: CFStringRef.self).pointee = Unmanaged.passRetained(str).takeRetainedValue()
            outSize.pointee = UInt32(MemoryLayout<CFStringRef>.size)
        case kAudioPlugInPropertyBundleID:
            let str = kPluginBundleID as CFString
            outPtr.assumingMemoryBound(to: CFStringRef.self).pointee = Unmanaged.passRetained(str).takeRetainedValue()
            outSize.pointee = UInt32(MemoryLayout<CFStringRef>.size)
        case kAudioPlugInPropertyDeviceList:
            let devPtr = outPtr.assumingMemoryBound(to: AudioObjectID.self)
            devPtr[0] = VisibleInputEndpoint.deviceObjectID
            devPtr[1] = HiddenOutputEndpoint.deviceObjectID
            outSize.pointee = UInt32(MemoryLayout<AudioObjectID>.size * 2)
        case kAudioPlugInPropertyTranslateUIDToDevice:
            guard inQualifierDataSize >= MemoryLayout<CFStringRef>.size, let qPtr = inQualifierData else {
                return kAudioHardwareBadPropertySizeError
            }
            let uid = qPtr.assumingMemoryBound(to: CFString.self).pointee as String
            if uid == VisibleInputEndpoint.deviceUID {
                outPtr.assumingMemoryBound(to: AudioObjectID.self).pointee = VisibleInputEndpoint.deviceObjectID
            } else if uid == HiddenOutputEndpoint.deviceUID {
                outPtr.assumingMemoryBound(to: AudioObjectID.self).pointee = HiddenOutputEndpoint.deviceObjectID
            } else {
                outPtr.assumingMemoryBound(to: AudioObjectID.self).pointee = kAudioObjectUnknown
            }
            outSize.pointee = UInt32(MemoryLayout<AudioObjectID>.size)
        default:
            return kAudioHardwareUnknownPropertyError
        }
        return noErr
    case VisibleInputEndpoint.deviceObjectID:
        return RealtimeNoiseDriver.shared.visibleInput.getPropertyData(
            address: addr,
            outDataSize: &outSize.pointee,
            outData: outPtr
        )
    case HiddenOutputEndpoint.deviceObjectID:
        return RealtimeNoiseDriver.shared.hiddenOutput.getPropertyData(
            address: addr,
            outDataSize: &outSize.pointee,
            outData: outPtr
        )
    default:
        return kAudioHardwareBadObjectError
    }
}

private func Driver_SetPropertyData(
    inDriver: AudioServerPlugInDriverRef?,
    inObjectID: AudioObjectID,
    inClientProcessID: pid_t,
    inAddress: UnsafePointer<AudioObjectPropertyAddress>?,
    inQualifierDataSize: UInt32,
    inQualifierData: UnsafeRawPointer?,
    inDataSize: UInt32,
    inData: UnsafeRawPointer?
) -> OSStatus {
    guard let addr = inAddress?.pointee, let inData = inData else {
        return kAudioHardwareIllegalOperationError
    }

    switch inObjectID {
    case VisibleInputEndpoint.deviceObjectID:
        return RealtimeNoiseDriver.shared.visibleInput.setPropertyData(
            address: addr,
            inDataSize: inDataSize,
            inData: inData
        )
    default:
        return kAudioHardwareUnknownPropertyError
    }
}

private func Driver_StartIO(
    inDriver: AudioServerPlugInDriverRef?,
    inDeviceObjectID: AudioObjectID,
    inClientID: UInt32
) -> OSStatus {
    if inDeviceObjectID == VisibleInputEndpoint.deviceObjectID {
        return RealtimeNoiseDriver.shared.visibleInput.startIO()
    } else if inDeviceObjectID == HiddenOutputEndpoint.deviceObjectID {
        return RealtimeNoiseDriver.shared.hiddenOutput.startIO(clientPID: pid_t(inClientID))
    }
    return kAudioHardwareBadObjectError
}

private func Driver_StopIO(
    inDriver: AudioServerPlugInDriverRef?,
    inDeviceObjectID: AudioObjectID,
    inClientID: UInt32
) -> OSStatus {
    if inDeviceObjectID == VisibleInputEndpoint.deviceObjectID {
        return RealtimeNoiseDriver.shared.visibleInput.stopIO()
    } else if inDeviceObjectID == HiddenOutputEndpoint.deviceObjectID {
        return RealtimeNoiseDriver.shared.hiddenOutput.stopIO(clientPID: pid_t(inClientID))
    }
    return kAudioHardwareBadObjectError
}

private func Driver_GetZeroTimeStamp(
    inDriver: AudioServerPlugInDriverRef?,
    inDeviceObjectID: AudioObjectID,
    inClientID: UInt32,
    outSampleTime: UnsafeMutablePointer<Float64>?,
    outHostTime: UnsafeMutablePointer<UInt64>?,
    outSeed: UnsafeMutablePointer<UInt64>?
) -> OSStatus {
    guard let outSample = outSampleTime, let outHost = outHostTime, let outSeed = outSeed else {
        return kAudioHardwareIllegalOperationError
    }

    let hostNow = mach_absolute_time()
    outHost.pointee = hostNow
    outSample.pointee = Float64(hostNow) / 1000.0
    outSeed.pointee = 1
    return noErr
}

private func Driver_WillDoIOOperation(
    inDriver: AudioServerPlugInDriverRef?,
    inDeviceObjectID: AudioObjectID,
    inClientID: UInt32,
    inOperationID: UInt32,
    outWillDo: UnsafeMutablePointer<DarwinBoolean>?,
    outWillDoInPlace: UnsafeMutablePointer<DarwinBoolean>?
) -> OSStatus {
    guard let outWillDo = outWillDo, let outWillDoInPlace = outWillDoInPlace else {
        return kAudioHardwareIllegalOperationError
    }

    if inDeviceObjectID == VisibleInputEndpoint.deviceObjectID {
        if inOperationID == kAudioServerPlugInIOOperationReadInput {
            outWillDo.pointee = DarwinBoolean(true)
            outWillDoInPlace.pointee = DarwinBoolean(true)
            return noErr
        }
    } else if inDeviceObjectID == HiddenOutputEndpoint.deviceObjectID {
        if inOperationID == kAudioServerPlugInIOOperationWriteMix {
            outWillDo.pointee = DarwinBoolean(true)
            outWillDoInPlace.pointee = DarwinBoolean(true)
            return noErr
        }
    }

    outWillDo.pointee = DarwinBoolean(false)
    outWillDoInPlace.pointee = DarwinBoolean(true)
    return noErr
}

private func Driver_BeginIOOperation(
    inDriver: AudioServerPlugInDriverRef?,
    inDeviceObjectID: AudioObjectID,
    inClientID: UInt32,
    inOperationID: UInt32,
    inIOBufferFrameSize: UInt32,
    inIOCycleInfo: UnsafePointer<AudioServerPlugInIOCycleInfo>?
) -> OSStatus {
    return noErr
}

/// Realtime I/O dispatching.
/// Zero heap allocations, zero blocking locks, zero synchronous RPCs.
private func Driver_DoIOOperation(
    inDriver: AudioServerPlugInDriverRef?,
    inDeviceObjectID: AudioObjectID,
    inStreamObjectID: AudioObjectID,
    inClientID: UInt32,
    inOperationID: UInt32,
    inIOBufferFrameSize: UInt32,
    inIOCycleInfo: UnsafePointer<AudioServerPlugInIOCycleInfo>?,
    ioMainBuffer: UnsafeMutableRawPointer?,
    ioSecondaryBuffer: UnsafeMutableRawPointer?
) -> OSStatus {
    guard let mainBuffer = ioMainBuffer else { return kAudioHardwareIllegalOperationError }

    let driver = RealtimeNoiseDriver.shared

    if inDeviceObjectID == VisibleInputEndpoint.deviceObjectID {
        if inOperationID == kAudioServerPlugInIOOperationReadInput {
            return driver.visibleInput.readInput(
                destination: mainBuffer,
                frameCount: inIOBufferFrameSize,
                ringBuffer: driver.ringBuffer
            )
        }
    } else if inDeviceObjectID == HiddenOutputEndpoint.deviceObjectID {
        if inOperationID == kAudioServerPlugInIOOperationWriteMix {
            let gen = driver.visibleInput.getExpectedGeneration()
            return driver.hiddenOutput.writeOutput(
                buffer: mainBuffer,
                frameCount: inIOBufferFrameSize,
                clientPID: pid_t(inClientID),
                generation: gen,
                ringBuffer: driver.ringBuffer
            )
        }
    }

    return noErr
}

private func Driver_EndIOOperation(
    inDriver: AudioServerPlugInDriverRef?,
    inDeviceObjectID: AudioObjectID,
    inClientID: UInt32,
    inOperationID: UInt32,
    inIOBufferFrameSize: UInt32,
    inIOCycleInfo: UnsafePointer<AudioServerPlugInIOCycleInfo>?
) -> OSStatus {
    return noErr
}
