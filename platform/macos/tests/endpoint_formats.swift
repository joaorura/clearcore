#!/usr/bin/env swift
//===----------------------------------------------------------------------===//
//
// endpoint_formats.swift
// RealtimeNoiseHAL Integration Test Suite
//
// Validates 48 kHz Float32 mono format negotiation and rejecting unsupported
// sample rates and channel configurations without crashing or memory corruption.
//
//===----------------------------------------------------------------------===//

import Foundation

#if canImport(CoreAudio)
import CoreAudio
#else
public typealias OSStatus = Int32
public let noErr: OSStatus = 0
public let kAudioDeviceUnsupportedFormatError: OSStatus = 0x21646174 // '!dat'
public let kAudioHardwareIllegalOperationError: OSStatus = 0x77686f3f // 'who?'
public let kAudioHardwareBadPropertySizeError: OSStatus = 0x2173697a // '!siz'

public struct AudioStreamBasicDescription: Equatable {
    public var mSampleRate: Float64
    public var mFormatID: UInt32
    public var mFormatFlags: UInt32
    public var mBytesPerPacket: UInt32
    public var mFramesPerPacket: UInt32
    public var mBytesPerFrame: UInt32
    public var mChannelsPerFrame: UInt32
    public var mBitsPerChannel: UInt32
    public var mReserved: UInt32

    public init(
        mSampleRate: Float64,
        mFormatID: UInt32,
        mFormatFlags: UInt32,
        mBytesPerPacket: UInt32,
        mFramesPerPacket: UInt32,
        mBytesPerFrame: UInt32,
        mChannelsPerFrame: UInt32,
        mBitsPerChannel: UInt32,
        mReserved: UInt32 = 0
    ) {
        self.mSampleRate = mSampleRate
        self.mFormatID = mFormatID
        self.mFormatFlags = mFormatFlags
        self.mBytesPerPacket = mBytesPerPacket
        self.mFramesPerPacket = mFramesPerPacket
        self.mBytesPerFrame = mBytesPerFrame
        self.mChannelsPerFrame = mChannelsPerFrame
        self.mBitsPerChannel = mBitsPerChannel
        self.mReserved = mReserved
    }
}

public let kAudioFormatLinearPCM: UInt32 = 0x6c70636d // 'lpcm'
public let kAudioFormatFlagIsFloat: UInt32 = (1 << 0)
public let kAudioFormatFlagIsPacked: UInt32 = (1 << 3)
#endif

// MARK: - Endpoint Format Validator

public final class EndpointFormatValidator {
    public static let standardSampleRate: Float64 = 48000.0
    public static let standardChannels: UInt32 = 1
    public static let standardBitsPerChannel: UInt32 = 32
    public static let minBufferFrameSize: UInt32 = 64
    public static let maxBufferFrameSize: UInt32 = 4096
    public static let defaultBufferFrameSize: UInt32 = 480

    public static let expectedStreamFormat = AudioStreamBasicDescription(
        mSampleRate: standardSampleRate,
        mFormatID: kAudioFormatLinearPCM,
        mFormatFlags: kAudioFormatFlagIsFloat | kAudioFormatFlagIsPacked,
        mBytesPerPacket: 4,
        mFramesPerPacket: 1,
        mBytesPerFrame: 4,
        mChannelsPerFrame: standardChannels,
        mBitsPerChannel: standardBitsPerChannel,
        mReserved: 0
    )

    private var currentSampleRate: Float64 = standardSampleRate
    private var currentBufferSize: UInt32 = defaultBufferFrameSize

    public init() {}

    public func negotiateFormat(format: AudioStreamBasicDescription) -> OSStatus {
        guard format.mFormatID == kAudioFormatLinearPCM else {
            return kAudioDeviceUnsupportedFormatError
        }
        guard format.mSampleRate == Self.standardSampleRate else {
            return kAudioDeviceUnsupportedFormatError
        }
        guard format.mChannelsPerFrame == Self.standardChannels else {
            return kAudioDeviceUnsupportedFormatError
        }
        guard format.mBitsPerChannel == Self.standardBitsPerChannel else {
            return kAudioDeviceUnsupportedFormatError
        }
        guard (format.mFormatFlags & kAudioFormatFlagIsFloat) != 0 else {
            return kAudioDeviceUnsupportedFormatError
        }
        return noErr
    }

    public func setNominalSampleRate(_ rate: Float64) -> OSStatus {
        guard rate == Self.standardSampleRate else {
            return kAudioDeviceUnsupportedFormatError
        }
        currentSampleRate = rate
        return noErr
    }

    public func setBufferFrameSize(_ size: UInt32) -> OSStatus {
        guard size >= Self.minBufferFrameSize && size <= Self.maxBufferFrameSize else {
            return kAudioHardwareIllegalOperationError
        }
        currentBufferSize = size
        return noErr
    }
}

// MARK: - Test Runner

public final class EndpointFormatsIntegrationTests {
    public static func runAll() -> Bool {
        print("[TEST-SUITE] Running macOS Endpoint Formats Integration Tests...")
        var passed = true

        passed = passed && testSupportedEndpointFormatsMatch48kHzMonoFloat32()
        passed = passed && testRejectUnsupportedSampleRatesWithoutCrashing()
        passed = passed && testRejectUnsupportedChannelCountsWithoutCrashing()
        passed = passed && testBufferFrameSizeBoundaryValidation()

        if passed {
            print("[PASS] All Endpoint Formats Integration tests passed successfully.")
        } else {
            print("[FAIL] Endpoint Formats Integration tests failed!")
        }
        return passed
    }

    /// Validates 48 kHz Float32 mono format negotiation matches CoreAudio HAL standard.
    public static func testSupportedEndpointFormatsMatch48kHzMonoFloat32() -> Bool {
        print("  -> testSupportedEndpointFormatsMatch48kHzMonoFloat32")
        let validator = EndpointFormatValidator()

        let format = EndpointFormatValidator.expectedStreamFormat
        let status = validator.negotiateFormat(format: format)
        guard status == noErr else {
            print("     [FAIL] 48kHz Float32 mono format negotiation failed: \(status)")
            return false
        }

        let rateStatus = validator.setNominalSampleRate(48000.0)
        guard rateStatus == noErr else {
            print("     [FAIL] Nominal sample rate set to 48000.0 failed")
            return false
        }

        print("     [PASS] 48kHz Float32 mono format negotiation confirmed.")
        return true
    }

    /// Validates that unsupported sample rates (44.1k, 88.2k, 96k, 16k, 192k) are rejected cleanly.
    public static func testRejectUnsupportedSampleRatesWithoutCrashing() -> Bool {
        print("  -> testRejectUnsupportedSampleRatesWithoutCrashing")
        let validator = EndpointFormatValidator()

        let unsupportedRates: [Float64] = [
            8000.0,
            16000.0,
            44100.0,
            88200.0,
            96000.0,
            176400.0,
            192000.0,
            384000.0
        ]

        for rate in unsupportedRates {
            let status = validator.setNominalSampleRate(rate)
            guard status == kAudioDeviceUnsupportedFormatError else {
                print("     [FAIL] Unsupported rate \(rate) was not rejected with kAudioDeviceUnsupportedFormatError (got \(status))")
                return false
            }

            var format = EndpointFormatValidator.expectedStreamFormat
            format.mSampleRate = rate
            let fmtStatus = validator.negotiateFormat(format: format)
            guard fmtStatus == kAudioDeviceUnsupportedFormatError else {
                print("     [FAIL] Format with sample rate \(rate) was not rejected (got \(fmtStatus))")
                return false
            }
        }

        print("     [PASS] All unsupported sample rates cleanly rejected without crashing.")
        return true
    }

    /// Validates that unsupported channel configurations (Stereo, Quad, 5.1, 7.1) are rejected cleanly.
    public static func testRejectUnsupportedChannelCountsWithoutCrashing() -> Bool {
        print("  -> testRejectUnsupportedChannelCountsWithoutCrashing")
        let validator = EndpointFormatValidator()

        let unsupportedChannels: [UInt32] = [0, 2, 4, 6, 8, 16]

        for ch in unsupportedChannels {
            var format = EndpointFormatValidator.expectedStreamFormat
            format.mChannelsPerFrame = ch
            format.mBytesPerFrame = ch * 4
            format.mBytesPerPacket = ch * 4

            let status = validator.negotiateFormat(format: format)
            guard status == kAudioDeviceUnsupportedFormatError else {
                print("     [FAIL] Unsupported channel count \(ch) was not rejected (got \(status))")
                return false
            }
        }

        print("     [PASS] All unsupported channel counts cleanly rejected without crashing.")
        return true
    }

    /// Validates buffer frame size boundary checks ([64, 4096]).
    public static func testBufferFrameSizeBoundaryValidation() -> Bool {
        print("  -> testBufferFrameSizeBoundaryValidation")
        let validator = EndpointFormatValidator()

        // Valid sizes
        let validSizes: [UInt32] = [64, 128, 256, 480, 512, 1024, 2048, 4096]
        for size in validSizes {
            let status = validator.setBufferFrameSize(size)
            guard status == noErr else {
                print("     [FAIL] Valid buffer size \(size) rejected: \(status)")
                return false
            }
        }

        // Invalid sizes (below min or above max)
        let invalidSizes: [UInt32] = [0, 16, 32, 63, 4097, 8192, 16384]
        for size in invalidSizes {
            let status = validator.setBufferFrameSize(size)
            guard status == kAudioHardwareIllegalOperationError else {
                print("     [FAIL] Invalid buffer size \(size) not rejected with kAudioHardwareIllegalOperationError (got \(status))")
                return false
            }
        }

        print("     [PASS] Buffer frame size boundaries correctly enforced.")
        return true
    }
}

// Direct executable invocation entry point
#if !TEST_HOST_STANDALONE
let status = EndpointFormatsIntegrationTests.runAll()
if !status {
    exit(1)
}
#endif
