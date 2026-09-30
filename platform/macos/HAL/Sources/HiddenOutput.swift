//===----------------------------------------------------------------------===//
//
// HiddenOutput.swift
// RealtimeNoiseHAL
//
// Hidden output endpoint where `realtime-noise-service` engine writes processed audio.
// Features:
// - Non-mixable private stream interface.
// - CoreAudio `kAudioDevicePropertyIsHidden = 1` preventing app accidental selection.
// - Enforces owner session lock: secondary client access rejected with `UnavailableBusy`.
// - Strictly zero memory allocation, zero blocking locks, zero synchronous RPCs in callback.
//
//===----------------------------------------------------------------------===//

import Foundation
import CoreAudio

#if canImport(Darwin)
import Darwin
#endif

/// FourCC error code representing `UnavailableBusy` ('busy' = 0x62757379).
public let kAudioHardwareUnavailableBusyError: OSStatus = 0x62757379

/// Hidden loopback output endpoint for `realtime-noise-service`.
public final class HiddenOutputEndpoint {
    // MARK: - CoreAudio Object Identifiers

    public static let deviceObjectID: AudioObjectID = 0x2000
    public static let streamObjectID: AudioObjectID = 0x2001
    public static let volumeControlObjectID: AudioObjectID = 0x2002
    public static let muteControlObjectID: AudioObjectID = 0x2003

    public static let deviceUID = "com.clearcore.realtime-noise.endpoint.hidden-output"
    public static let modelUID = "com.clearcore.realtime-noise.model.hidden-output"
    public static let deviceName = "Clearcore Engine Loopback Output (Hidden)"
    public static let manufacturer = "CLRC"
    public static let subtype = "rtns"

    // MARK: - Audio Stream Basic Description (48kHz Float32 Mono)

    public static let sampleRate: Float64 = 48000.0
    public static let channelCount: UInt32 = 1
    public static let bytesPerSample: UInt32 = UInt32(MemoryLayout<Float32>.size)
    public static let defaultBufferSize: UInt32 = 480 // 10ms frame size

    public static let streamFormat = AudioStreamBasicDescription(
        mSampleRate: sampleRate,
        mFormatID: kAudioFormatLinearPCM,
        mFormatFlags: kAudioFormatFlagIsFloat | kAudioFormatFlagIsPacked,
        mBytesPerPacket: bytesPerSample * channelCount,
        mFramesPerPacket: 1,
        mBytesPerFrame: bytesPerSample * channelCount,
        mChannelsPerFrame: channelCount,
        mBitsPerChannel: 32,
        mReserved: 0
    )

    // MARK: - Realtime Atomic State (Pre-allocated, Lock-Free)

    private let ownerProcessIDPtr: UnsafeMutablePointer<pid_t>
    private let isOwnedPtr: UnsafeMutablePointer<UInt32>
    private let isRunningPtr: UnsafeMutablePointer<UInt32>
    private let sequenceCounterPtr: UnsafeMutablePointer<UInt64>
    private let bufferFrameSizePtr: UnsafeMutablePointer<UInt32>

    // Control-plane lock (used ONLY out-of-callback)
    private let controlLock = os_unfair_lock_t.allocate(capacity: 1)

    public init() {
        controlLock.initialize(to: os_unfair_lock())

        ownerProcessIDPtr = UnsafeMutablePointer<pid_t>.allocate(capacity: 1)
        ownerProcessIDPtr.initialize(to: 0)

        isOwnedPtr = UnsafeMutablePointer<UInt32>.allocate(capacity: 1)
        isOwnedPtr.initialize(to: 0)

        isRunningPtr = UnsafeMutablePointer<UInt32>.allocate(capacity: 1)
        isRunningPtr.initialize(to: 0)

        sequenceCounterPtr = UnsafeMutablePointer<UInt64>.allocate(capacity: 1)
        sequenceCounterPtr.initialize(to: 0)

        bufferFrameSizePtr = UnsafeMutablePointer<UInt32>.allocate(capacity: 1)
        bufferFrameSizePtr.initialize(to: Self.defaultBufferSize)
    }

    deinit {
        controlLock.deallocate()
        ownerProcessIDPtr.deallocate()
        isOwnedPtr.deallocate()
        isRunningPtr.deallocate()
        sequenceCounterPtr.deallocate()
        bufferFrameSizePtr.deallocate()
    }

    // MARK: - Memory Ordering Primitives

    @inline(__always)
    private func memoryBarrier() {
        #if canImport(Darwin)
        OSMemoryBarrier()
        #endif
    }

    // MARK: - Owner Session Lock Management (Out-of-Callback)

    /// Acquires exclusive ownership for the noise suppression engine writer.
    /// Returns `kAudioHardwareUnavailableBusyError` if another process holds ownership.
    public func acquireOwnerLock(clientPID: pid_t) -> OSStatus {
        os_unfair_lock_lock(controlLock)
        defer { os_unfair_lock_unlock(controlLock) }

        if isOwnedPtr.pointee != 0 && ownerProcessIDPtr.pointee != clientPID {
            // Non-owner session receives UnavailableBusy
            return kAudioHardwareUnavailableBusyError
        }

        memoryBarrier()
        isOwnedPtr.pointee = 1
        ownerProcessIDPtr.pointee = clientPID
        memoryBarrier()
        return noErr
    }

    /// Releases exclusive ownership when engine disconnects or terminates.
    public func releaseOwnerLock(clientPID: pid_t) -> OSStatus {
        os_unfair_lock_lock(controlLock)
        defer { os_unfair_lock_unlock(controlLock) }

        if isOwnedPtr.pointee != 0 && ownerProcessIDPtr.pointee == clientPID {
            memoryBarrier()
            isOwnedPtr.pointee = 0
            ownerProcessIDPtr.pointee = 0
            memoryBarrier()
        }
        return noErr
    }

    /// Verifies whether the specified client PID is the current owner.
    public func isOwner(clientPID: pid_t) -> Bool {
        memoryBarrier()
        return isOwnedPtr.pointee == 0 || ownerProcessIDPtr.pointee == clientPID
    }

    // MARK: - Device Lifecycle

    public func startIO(clientPID: pid_t) -> OSStatus {
        let status = acquireOwnerLock(clientPID: clientPID)
        guard status == noErr else { return status }

        os_unfair_lock_lock(controlLock)
        defer { os_unfair_lock_unlock(controlLock) }
        memoryBarrier()
        isRunningPtr.pointee = 1
        return noErr
    }

    public func stopIO(clientPID: pid_t) -> OSStatus {
        os_unfair_lock_lock(controlLock)
        if ownerProcessIDPtr.pointee == clientPID {
            memoryBarrier()
            isRunningPtr.pointee = 0
        }
        os_unfair_lock_unlock(controlLock)
        return releaseOwnerLock(clientPID: clientPID)
    }

    // MARK: - Realtime Audio Callback (Zero Allocation, Lock-Free, RPC-Free)

    /// Called during `kAudioServerPlugInIOOperationWriteMix`.
    /// Zero heap allocations, zero blocking locks, zero synchronous RPCs.
    @inline(__always)
    public func writeOutput(
        buffer: UnsafeRawPointer,
        frameCount: UInt32,
        clientPID: pid_t,
        generation: UInt64,
        ringBuffer: RingBuffer
    ) -> OSStatus {
        // Fast lock-free check of owner PID
        memoryBarrier()
        if isOwnedPtr.pointee != 0 && ownerProcessIDPtr.pointee != clientPID {
            return kAudioHardwareUnavailableBusyError
        }

        guard frameCount > 0 else { return noErr }
        let floatPtr = buffer.bindMemory(to: Float.self, capacity: Int(frameCount))

        sequenceCounterPtr.pointee &+= 1
        ringBuffer.write(
            samples: floatPtr,
            count: Int(frameCount),
            sequence: sequenceCounterPtr.pointee,
            generation: generation
        )

        return noErr
    }

    // MARK: - Property Dispatching

    public func hasProperty(address: AudioObjectPropertyAddress) -> Bool {
        switch address.mSelector {
        case kAudioObjectPropertyBaseClass,
             kAudioObjectPropertyClass,
             kAudioObjectPropertyOwner,
             kAudioObjectPropertyName,
             kAudioObjectPropertyManufacturer,
             kAudioObjectPropertyOwnedObjects,
             kAudioDevicePropertyDeviceUID,
             kAudioDevicePropertyModelUID,
             kAudioDevicePropertyTransportType,
             kAudioDevicePropertyStreams,
             kAudioDevicePropertyIsHidden,
             kAudioDevicePropertyDeviceIsAlive,
             kAudioDevicePropertyDeviceIsRunning,
             kAudioDevicePropertyNominalSampleRate,
             kAudioDevicePropertyAvailableNominalSampleRates,
             kAudioDevicePropertyBufferFrameSize,
             kAudioDevicePropertyBufferFrameSizeRange,
             kAudioDevicePropertyLatency,
             kAudioDevicePropertySafetyOffset:
            return true
        default:
            return false
        }
    }

    public func isPropertySettable(address: AudioObjectPropertyAddress) -> Bool {
        switch address.mSelector {
        case kAudioDevicePropertyBufferFrameSize,
             kAudioDevicePropertyNominalSampleRate:
            return true
        default:
            return false
        }
    }

    public func getPropertyDataSize(address: AudioObjectPropertyAddress) -> UInt32 {
        switch address.mSelector {
        case kAudioObjectPropertyBaseClass,
             kAudioObjectPropertyClass,
             kAudioObjectPropertyOwner,
             kAudioDevicePropertyTransportType,
             kAudioDevicePropertyIsHidden,
             kAudioDevicePropertyDeviceIsAlive,
             kAudioDevicePropertyDeviceIsRunning,
             kAudioDevicePropertyBufferFrameSize,
             kAudioDevicePropertyLatency,
             kAudioDevicePropertySafetyOffset:
            return UInt32(MemoryLayout<UInt32>.size)
        case kAudioDevicePropertyNominalSampleRate:
            return UInt32(MemoryLayout<Float64>.size)
        case kAudioDevicePropertyAvailableNominalSampleRates:
            return UInt32(MemoryLayout<AudioValueRange>.size)
        case kAudioDevicePropertyBufferFrameSizeRange:
            return UInt32(MemoryLayout<AudioValueRange>.size)
        case kAudioDevicePropertyStreams:
            return UInt32(MemoryLayout<AudioObjectID>.size)
        case kAudioObjectPropertyOwnedObjects:
            return UInt32(MemoryLayout<AudioObjectID>.size * 3) // Stream + Volume + Mute
        case kAudioObjectPropertyName,
             kAudioObjectPropertyManufacturer,
             kAudioDevicePropertyDeviceUID,
             kAudioDevicePropertyModelUID:
            return UInt32(MemoryLayout<CFStringRef>.size)
        default:
            return 0
        }
    }

    public func getPropertyData(
        address: AudioObjectPropertyAddress,
        outDataSize: inout UInt32,
        outData: UnsafeMutableRawPointer
    ) -> OSStatus {
        switch address.mSelector {
        case kAudioObjectPropertyBaseClass:
            outData.assumingMemoryBound(to: AudioClassID.self).pointee = kAudioObjectClassID
            outDataSize = UInt32(MemoryLayout<AudioClassID>.size)
        case kAudioObjectPropertyClass:
            outData.assumingMemoryBound(to: AudioClassID.self).pointee = kAudioDeviceClassID
            outDataSize = UInt32(MemoryLayout<AudioClassID>.size)
        case kAudioObjectPropertyName:
            let name = Self.deviceName as CFString
            outData.assumingMemoryBound(to: CFStringRef.self).pointee = Unmanaged.passRetained(name).takeRetainedValue()
            outDataSize = UInt32(MemoryLayout<CFStringRef>.size)
        case kAudioObjectPropertyManufacturer:
            let mfr = Self.manufacturer as CFString
            outData.assumingMemoryBound(to: CFStringRef.self).pointee = Unmanaged.passRetained(mfr).takeRetainedValue()
            outDataSize = UInt32(MemoryLayout<CFStringRef>.size)
        case kAudioDevicePropertyDeviceUID:
            let uid = Self.deviceUID as CFString
            outData.assumingMemoryBound(to: CFStringRef.self).pointee = Unmanaged.passRetained(uid).takeRetainedValue()
            outDataSize = UInt32(MemoryLayout<CFStringRef>.size)
        case kAudioDevicePropertyModelUID:
            let muid = Self.modelUID as CFString
            outData.assumingMemoryBound(to: CFStringRef.self).pointee = Unmanaged.passRetained(muid).takeRetainedValue()
            outDataSize = UInt32(MemoryLayout<CFStringRef>.size)
        case kAudioDevicePropertyTransportType:
            outData.assumingMemoryBound(to: UInt32.self).pointee = kAudioDeviceTransportTypeVirtual
            outDataSize = UInt32(MemoryLayout<UInt32>.size)
        case kAudioDevicePropertyIsHidden:
            // Non-mixable private hidden stream: MUST be 1
            outData.assumingMemoryBound(to: UInt32.self).pointee = 1
            outDataSize = UInt32(MemoryLayout<UInt32>.size)
        case kAudioDevicePropertyDeviceIsAlive:
            outData.assumingMemoryBound(to: UInt32.self).pointee = 1
            outDataSize = UInt32(MemoryLayout<UInt32>.size)
        case kAudioDevicePropertyDeviceIsRunning:
            outData.assumingMemoryBound(to: UInt32.self).pointee = isRunningPtr.pointee
            outDataSize = UInt32(MemoryLayout<UInt32>.size)
        case kAudioDevicePropertyNominalSampleRate:
            outData.assumingMemoryBound(to: Float64.self).pointee = Self.sampleRate
            outDataSize = UInt32(MemoryLayout<Float64>.size)
        case kAudioDevicePropertyAvailableNominalSampleRates:
            let range = AudioValueRange(mMinimum: Self.sampleRate, mMaximum: Self.sampleRate)
            outData.assumingMemoryBound(to: AudioValueRange.self).pointee = range
            outDataSize = UInt32(MemoryLayout<AudioValueRange>.size)
        case kAudioDevicePropertyBufferFrameSize:
            outData.assumingMemoryBound(to: UInt32.self).pointee = bufferFrameSizePtr.pointee
            outDataSize = UInt32(MemoryLayout<UInt32>.size)
        case kAudioDevicePropertyBufferFrameSizeRange:
            let range = AudioValueRange(mMinimum: 64, mMaximum: 4096)
            outData.assumingMemoryBound(to: AudioValueRange.self).pointee = range
            outDataSize = UInt32(MemoryLayout<AudioValueRange>.size)
        case kAudioDevicePropertyStreams:
            outData.assumingMemoryBound(to: AudioObjectID.self).pointee = Self.streamObjectID
            outDataSize = UInt32(MemoryLayout<AudioObjectID>.size)
        case kAudioObjectPropertyOwnedObjects:
            let objs = outData.assumingMemoryBound(to: AudioObjectID.self)
            objs[0] = Self.streamObjectID
            objs[1] = Self.volumeControlObjectID
            objs[2] = Self.muteControlObjectID
            outDataSize = UInt32(MemoryLayout<AudioObjectID>.size * 3)
        case kAudioDevicePropertyLatency, kAudioDevicePropertySafetyOffset:
            outData.assumingMemoryBound(to: UInt32.self).pointee = 0
            outDataSize = UInt32(MemoryLayout<UInt32>.size)
        default:
            return kAudioHardwareUnknownPropertyError
        }
        return noErr
    }

    public func setPropertyData(
        address: AudioObjectPropertyAddress,
        inDataSize: UInt32,
        inData: UnsafeRawPointer
    ) -> OSStatus {
        switch address.mSelector {
        case kAudioDevicePropertyBufferFrameSize:
            guard inDataSize >= MemoryLayout<UInt32>.size else { return kAudioHardwareBadPropertySizeError }
            let newSize = inData.assumingMemoryBound(to: UInt32.self).pointee
            guard newSize >= 64 && newSize <= 4096 else { return kAudioHardwareIllegalOperationError }
            memoryBarrier()
            bufferFrameSizePtr.pointee = newSize
            return noErr
        case kAudioDevicePropertyNominalSampleRate:
            guard inDataSize >= MemoryLayout<Float64>.size else { return kAudioHardwareBadPropertySizeError }
            let requestedRate = inData.assumingMemoryBound(to: Float64.self).pointee
            guard requestedRate == Self.sampleRate else {
                return kAudioDeviceUnsupportedFormatError
            }
            return noErr
        default:
            return kAudioHardwareUnknownPropertyError
        }
    }
}
