//===----------------------------------------------------------------------===//
//
// VisibleInput.swift
// RealtimeNoiseHAL
//
// Visible virtual microphone input exposed to applications via CoreAudio HAL (`AudioDeviceCreate`).
// Features:
// - Exposed to system as 48kHz Float32 mono microphone input.
// - Realtime I/O callback reads directly from `RingBuffer.swift`.
// - Strictly zero memory allocations, zero blocking locks, zero synchronous RPCs in callback.
// - Fail-closed: outputs digital silence (zeros) on underrun, generation mismatch, or absence.
// - Zero raw audio leakage: hardware microphone audio is never bridged directly.
//
//===----------------------------------------------------------------------===//

import Foundation
import CoreAudio

#if canImport(Darwin)
import Darwin
#endif

/// Visible virtual microphone input device selectable in user applications (Teams, Zoom, Discord, OBS).
public final class VisibleInputEndpoint {
    // MARK: - CoreAudio Object Identifiers

    public static let deviceObjectID: AudioObjectID = 0x1000
    public static let streamObjectID: AudioObjectID = 0x1001
    public static let volumeControlObjectID: AudioObjectID = 0x1002
    public static let muteControlObjectID: AudioObjectID = 0x1003

    public static let deviceUID = "com.clearcore.realtime-noise.endpoint.visible-input"
    public static let modelUID = "com.clearcore.realtime-noise.model.visible-input"
    public static let deviceName = "Clearcore Realtime Noise Suppression Microphone"
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

    private let expectedGenerationPtr: UnsafeMutablePointer<UInt64>
    private let volumePtr: UnsafeMutablePointer<Float32>
    private let isMutedPtr: UnsafeMutablePointer<UInt32>
    private let isRunningPtr: UnsafeMutablePointer<UInt32>
    private let bufferFrameSizePtr: UnsafeMutablePointer<UInt32>

    // Control-plane lock (used ONLY out-of-callback)
    private let controlLock = os_unfair_lock_t.allocate(capacity: 1)
    private var activeClientCount: Int = 0

    public init() {
        controlLock.initialize(to: os_unfair_lock())

        expectedGenerationPtr = UnsafeMutablePointer<UInt64>.allocate(capacity: 1)
        expectedGenerationPtr.initialize(to: 1)

        volumePtr = UnsafeMutablePointer<Float32>.allocate(capacity: 1)
        volumePtr.initialize(to: 1.0)

        isMutedPtr = UnsafeMutablePointer<UInt32>.allocate(capacity: 1)
        isMutedPtr.initialize(to: 0)

        isRunningPtr = UnsafeMutablePointer<UInt32>.allocate(capacity: 1)
        isRunningPtr.initialize(to: 0)

        bufferFrameSizePtr = UnsafeMutablePointer<UInt32>.allocate(capacity: 1)
        bufferFrameSizePtr.initialize(to: Self.defaultBufferSize)
    }

    deinit {
        controlLock.deallocate()
        expectedGenerationPtr.deallocate()
        volumePtr.deallocate()
        isMutedPtr.deallocate()
        isRunningPtr.deallocate()
        bufferFrameSizePtr.deallocate()
    }

    // MARK: - Memory Ordering Primitives

    @inline(__always)
    private func memoryBarrier() {
        #if canImport(Darwin)
        OSMemoryBarrier()
        #endif
    }

    // MARK: - Control Plane Operations (Out-of-Callback)

    public func setExpectedGeneration(_ gen: UInt64) {
        memoryBarrier()
        expectedGenerationPtr.pointee = gen
        memoryBarrier()
    }

    /// Lock-free expected generation reader safe for realtime audio callback.
    @inline(__always)
    public func getExpectedGeneration() -> UInt64 {
        memoryBarrier()
        return expectedGenerationPtr.pointee
    }

    public func setMuted(_ muted: Bool) {
        memoryBarrier()
        isMutedPtr.pointee = muted ? 1 : 0
        memoryBarrier()
    }

    public func isMuted() -> Bool {
        memoryBarrier()
        return isMutedPtr.pointee != 0
    }

    public func setVolume(_ vol: Float32) {
        memoryBarrier()
        volumePtr.pointee = max(0.0, min(2.0, vol))
        memoryBarrier()
    }

    public func getVolume() -> Float32 {
        memoryBarrier()
        return volumePtr.pointee
    }

    public var activeClients: Int {
        os_unfair_lock_lock(controlLock)
        defer { os_unfair_lock_unlock(controlLock) }
        return activeClientCount
    }

    /// Observer callback invoked outside audio thread whenever activeClientCount changes.
    /// Parameters: (newActiveClientCount, isRunning)
    public var onActiveClientCountChange: ((Int, Bool) -> Void)?

    public func startIO(host: AudioServerPlugInHostRef? = nil) -> OSStatus {
        os_unfair_lock_lock(controlLock)
        let previousRunning = (activeClientCount > 0)
        activeClientCount += 1
        let count = activeClientCount
        let runningChanged = !previousRunning
        if runningChanged {
            memoryBarrier()
            isRunningPtr.pointee = 1
        }
        let observer = onActiveClientCountChange
        os_unfair_lock_unlock(controlLock)

        observer?(count, true)

        if runningChanged, let host = host {
            notifyDeviceIsRunningChanged(host: host)
        }

        return noErr
    }

    public func stopIO(host: AudioServerPlugInHostRef? = nil) -> OSStatus {
        os_unfair_lock_lock(controlLock)
        let previousRunning = (activeClientCount > 0)
        activeClientCount = max(0, activeClientCount - 1)
        let count = activeClientCount
        let runningChanged = previousRunning && (count == 0)
        if runningChanged {
            memoryBarrier()
            isRunningPtr.pointee = 0
        }
        let observer = onActiveClientCountChange
        os_unfair_lock_unlock(controlLock)

        observer?(count, count > 0)

        if runningChanged, let host = host {
            notifyDeviceIsRunningChanged(host: host)
        }

        return noErr
    }

    /// Notifies CoreAudio HAL host that `kAudioDevicePropertyDeviceIsRunning` has changed.
    public func notifyDeviceIsRunningChanged(host: AudioServerPlugInHostRef) {
        var address = AudioObjectPropertyAddress(
            mSelector: kAudioDevicePropertyDeviceIsRunning,
            mScope: kAudioObjectPropertyScopeGlobal,
            mElement: kAudioObjectPropertyElementMaster
        )
        #if canImport(Darwin)
        let hostInterface = host.pointee.pointee
        _ = withUnsafePointer(to: &address) { addrPtr in
            hostInterface.PropertiesChanged?(host, Self.deviceObjectID, 1, addrPtr)
        }
        #endif
    }

    // MARK: - Realtime Audio Callback (Zero Allocation, Lock-Free, RPC-Free)

    /// CoreAudio HAL I/O callback for `kAudioServerPlugInIOOperationReadInput`.
    /// Reads exclusively from `RingBuffer`.
    /// Zero heap allocations, zero blocking locks, zero synchronous RPCs.
    /// In case of underflow, absence, or un-warmed generation: outputs digital silence (zeros).
    @inline(__always)
    public func readInput(
        destination: UnsafeMutableRawPointer,
        frameCount: UInt32,
        ringBuffer: RingBuffer
    ) -> OSStatus {
        guard frameCount > 0 else { return noErr }

        let floatBuffer = destination.bindMemory(to: Float.self, capacity: Int(frameCount))
        let gen = expectedGenerationPtr.pointee
        memoryBarrier()

        // Fast lock-free mute check
        if isMutedPtr.pointee != 0 {
            floatBuffer.initialize(repeating: 0.0, count: Int(frameCount))
            return noErr
        }

        // Direct lock-free read from ring buffer (fail-closed inside RingBuffer on underrun)
        let framesRead = ringBuffer.read(
            into: floatBuffer,
            count: Int(frameCount),
            expectedGeneration: gen
        )

        // Apply volume scaling lock-free in-place without dynamic memory allocation
        let currentVol = volumePtr.pointee
        if framesRead > 0 && currentVol != 1.0 {
            for i in 0..<framesRead {
                floatBuffer[i] *= currentVol
            }
        }

        return noErr
    }

    // MARK: - CoreAudio Property Queries

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
            // Visible virtual input microphone: MUST be 0
            outData.assumingMemoryBound(to: UInt32.self).pointee = 0
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
            // Strictly enforce 48kHz Float32 mono contract: reject unsupported sample rates
            guard requestedRate == Self.sampleRate else {
                return kAudioDeviceUnsupportedFormatError
            }
            return noErr
        default:
            return kAudioHardwareUnknownPropertyError
        }
    }
}
