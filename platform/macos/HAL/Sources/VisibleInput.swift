//===----------------------------------------------------------------------===//
//
// VisibleInput.swift
// RealtimeNoiseHAL
//
// Visible virtual microphone input exposed to applications via CoreAudio HAL (`AudioDeviceCreate`).
// Features:
// - Exposed to system as 48kHz Float32 mono microphone input.
// - Realtime I/O callback reads directly from `RingBuffer.swift`.
// - Zero memory allocation, zero blocking locks, zero synchronous RPCs in callback.
// - Fail-closed: outputs digital silence (zeros) on underrun or absence.
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

    // MARK: - Internal State

    private let stateLock = os_unfair_lock_t.allocate(capacity: 1)
    private var isRunning: Bool = false
    private var activeClientCount: Int = 0
    private var volume: Float32 = 1.0
    private var isMuted: UInt32 = 0
    private var currentExpectedGeneration: UInt64 = 1

    public init() {
        stateLock.initialize(to: os_unfair_lock())
    }

    deinit {
        stateLock.deallocate()
    }

    // MARK: - Control Plane Operations (Out-of-Callback)

    public func setExpectedGeneration(_ gen: UInt64) {
        os_unfair_lock_lock(stateLock)
        currentExpectedGeneration = gen
        os_unfair_lock_unlock(stateLock)
    }

    public func getExpectedGeneration() -> UInt64 {
        os_unfair_lock_lock(stateLock)
        defer { os_unfair_lock_unlock(stateLock) }
        return currentExpectedGeneration
    }

    public func startIO() -> OSStatus {
        os_unfair_lock_lock(stateLock)
        defer { os_unfair_lock_unlock(stateLock) }
        activeClientCount += 1
        isRunning = true
        return noErr
    }

    public func stopIO() -> OSStatus {
        os_unfair_lock_lock(stateLock)
        defer { os_unfair_lock_unlock(stateLock) }
        activeClientCount = max(0, activeClientCount - 1)
        if activeClientCount == 0 {
            isRunning = false
        }
        return noErr
    }

    // MARK: - Realtime Audio Callback (Zero Allocation, Lock-Free)

    /// CoreAudio HAL I/O callback for `kAudioServerPlugInIOOperationReadInput`.
    /// Reads exclusively from `RingBuffer`.
    /// Zero heap allocations, zero blocking locks, zero synchronous RPCs.
    /// In case of underflow or un-warmed generation: outputs digital silence (zeros).
    @inline(__always)
    public func readInput(
        destination: UnsafeMutableRawPointer,
        frameCount: UInt32,
        ringBuffer: RingBuffer
    ) -> OSStatus {
        guard frameCount > 0 else { return noErr }

        let floatBuffer = destination.bindMemory(to: Float.self, capacity: Int(frameCount))
        let gen = currentExpectedGeneration

        // Check hardware/software mute flag
        if isMuted != 0 {
            floatBuffer.initialize(repeating: 0.0, count: Int(frameCount))
            return noErr
        }

        // Direct lock-free read from ring buffer (fail-closed inside RingBuffer on underrun)
        let framesRead = ringBuffer.read(
            into: floatBuffer,
            count: Int(frameCount),
            expectedGeneration: gen
        )

        // If ring buffer underruns, silence has already been populated
        if framesRead > 0 && volume != 1.0 {
            // Apply volume scaling in-place without memory allocation
            let gain = volume
            for i in 0..<framesRead {
                floatBuffer[i] *= gain
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
            outData.assumingMemoryBound(to: UInt32.self).pointee = isRunning ? 1 : 0
            outDataSize = UInt32(MemoryLayout<UInt32>.size)
        case kAudioDevicePropertyNominalSampleRate:
            outData.assumingMemoryBound(to: Float64.self).pointee = Self.sampleRate
            outDataSize = UInt32(MemoryLayout<Float64>.size)
        case kAudioDevicePropertyAvailableNominalSampleRates:
            let range = AudioValueRange(mMinimum: Self.sampleRate, mMaximum: Self.sampleRate)
            outData.assumingMemoryBound(to: AudioValueRange.self).pointee = range
            outDataSize = UInt32(MemoryLayout<AudioValueRange>.size)
        case kAudioDevicePropertyBufferFrameSize:
            outData.assumingMemoryBound(to: UInt32.self).pointee = Self.defaultBufferSize
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
            return noErr
        default:
            return kAudioHardwareUnknownPropertyError
        }
    }
}
