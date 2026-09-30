#!/usr/bin/env swift
//===----------------------------------------------------------------------===//
//
// hotplug.swift
// RealtimeNoiseHAL Integration Test Suite
//
// Validates that hardware audio device disconnection transitions audio state
// safely into digital silence without selecting alternative input devices
// without explicit user consent (Zero Raw Audio Leakage Guarantee).
//
//===----------------------------------------------------------------------===//

import Foundation

#if canImport(CoreAudio)
import CoreAudio
#endif

// MARK: - Hotplug Policy & Mock Audio Subsystem

public enum AudioDeviceState: String, CustomStringConvertible, Sendable {
    case active = "Active"
    case disconnectedWaitingForConsent = "DisconnectedWaitingForConsent"
    case silentFallbackPrevented = "SilentFallbackPrevented"

    public var description: String { rawValue }
}

public struct AudioDeviceDescription: Equatable, Sendable {
    public let uid: String
    public let name: String
    public let isBuiltIn: Bool
    public var isConnected: Bool
}

/// Manages device selection adhering strictly to the Clearcore Privacy & Consent policy.
/// Contract: If the selected input device is lost/disconnected, the audio pipeline MUST
/// output pure digital silence and MUST NEVER automatically fall back to an unselected device
/// (such as the internal laptop microphone) without explicit user authorization.
public final class HostDeviceArbitrator {
    public private(set) var availableDevices: [String: AudioDeviceDescription] = [:]
    public private(set) var selectedDeviceUID: String?
    public private(set) var currentState: AudioDeviceState = .disconnectedWaitingForConsent
    public private(set) var silenceFrameCount: UInt64 = 0

    public init() {}

    public func registerDevice(_ device: AudioDeviceDescription) {
        availableDevices[device.uid] = device
    }

    public func selectDevice(uid: String, explicitUserConsent: Bool) -> Bool {
        guard explicitUserConsent else {
            return false
        }
        guard let dev = availableDevices[uid], dev.isConnected else {
            return false
        }
        selectedDeviceUID = uid
        currentState = .active
        return true
    }

    /// Simulates a CoreAudio `kAudioHardwarePropertyDevices` notification when a device is unplugged.
    public func handleDeviceDisconnection(uid: String) {
        if var dev = availableDevices[uid] {
            dev.isConnected = false
            availableDevices[uid] = dev
        }

        if selectedDeviceUID == uid {
            // Selected device was lost!
            // CRITICAL CONTRACT: Do NOT select another device (e.g. built-in mic).
            // Transition state to silence without alternative selection.
            currentState = .disconnectedWaitingForConsent
        }
    }

    /// Produces audio frames for the visible virtual microphone.
    /// Under disconnected or consent-waiting state, fills buffer with pure digital silence (0.0).
    public func renderAudio(into buffer: UnsafeMutablePointer<Float>, frameCount: Int) {
        switch currentState {
        case .active:
            // Simulate processing denoised microphone input
            buffer.initialize(repeating: 0.42, count: frameCount)
        case .disconnectedWaitingForConsent, .silentFallbackPrevented:
            // Fail-closed digital silence
            buffer.initialize(repeating: 0.0, count: frameCount)
            silenceFrameCount &+= UInt64(frameCount)
        }
    }
}

// MARK: - Test Runner

public final class HotplugIntegrationTests {
    public static func runAll() -> Bool {
        print("[TEST-SUITE] Running macOS Hotplug Integration Tests...")
        var passed = true

        passed = passed && testDeviceDisconnectionTransitionsToSilenceWithoutAlternativeSelection()
        passed = passed && testUnauthorizedFallbackIsRejected()
        passed = passed && testReconnectionWithConsentRestoresStreaming()

        if passed {
            print("[PASS] All Hotplug Integration tests passed successfully.")
        } else {
            print("[FAIL] Hotplug Integration tests failed!")
        }
        return passed
    }

    /// Verifies that unplugging the active USB headset causes the pipeline to output silence,
    /// and does NOT automatically switch to the built-in laptop mic.
    public static func testDeviceDisconnectionTransitionsToSilenceWithoutAlternativeSelection() -> Bool {
        print("  -> testDeviceDisconnectionTransitionsToSilenceWithoutAlternativeSelection")
        let arbitrator = HostDeviceArbitrator()

        let usbHeadsetUID = "com.clearcore.audio.usb.headset-01"
        let builtInMicUID = "com.apple.audio.builtin-mic"

        arbitrator.registerDevice(AudioDeviceDescription(
            uid: usbHeadsetUID,
            name: "Studio USB Headset Microphone",
            isBuiltIn: false,
            isConnected: true
        ))

        arbitrator.registerDevice(AudioDeviceDescription(
            uid: builtInMicUID,
            name: "MacBook Pro Microphone",
            isBuiltIn: true,
            isConnected: true
        ))

        // Explicit user selects USB Headset
        let selectSuccess = arbitrator.selectDevice(uid: usbHeadsetUID, explicitUserConsent: true)
        guard selectSuccess, arbitrator.currentState == .active else {
            print("     [FAIL] Failed to select initial USB headset")
            return false
        }

        // Verify active audio rendering
        let frameCount = 480
        let audioBuffer = UnsafeMutablePointer<Float>.allocate(capacity: frameCount)
        defer { audioBuffer.deallocate() }

        arbitrator.renderAudio(into: audioBuffer, frameCount: frameCount)
        guard audioBuffer.pointee == 0.42 else {
            print("     [FAIL] Initial audio frames were not rendered")
            return false
        }

        // Simulate USB unplug event
        arbitrator.handleDeviceDisconnection(uid: usbHeadsetUID)

        // ASSERTION 1: State must be disconnected / waiting for consent
        guard arbitrator.currentState == .disconnectedWaitingForConsent else {
            print("     [FAIL] State after disconnection was \(arbitrator.currentState), expected DisconnectedWaitingForConsent")
            return false
        }

        // ASSERTION 2: Selected device must NOT have switched to built-in mic
        guard arbitrator.selectedDeviceUID != builtInMicUID else {
            print("     [FAIL] PRIVACY VIOLATION: Subsystem silently switched to built-in mic without user consent!")
            return false
        }

        // ASSERTION 3: Rendered audio must be pure digital silence (all zeros)
        audioBuffer.initialize(repeating: 99.0, count: frameCount)
        arbitrator.renderAudio(into: audioBuffer, frameCount: frameCount)

        for i in 0..<frameCount {
            if audioBuffer[i] != 0.0 {
                print("     [FAIL] Non-zero sample detected during disconnection at index \(i): \(audioBuffer[i])")
                return false
            }
        }

        print("     [PASS] Device disconnection safely transitions to silence without silent fallback.")
        return true
    }

    /// Verifies that attempting to switch to an alternative device without user consent is blocked.
    public static func testUnauthorizedFallbackIsRejected() -> Bool {
        print("  -> testUnauthorizedFallbackIsRejected")
        let arbitrator = HostDeviceArbitrator()
        let builtInMicUID = "com.apple.audio.builtin-mic"

        arbitrator.registerDevice(AudioDeviceDescription(
            uid: builtInMicUID,
            name: "MacBook Pro Microphone",
            isBuiltIn: true,
            isConnected: true
        ))

        // Attempting to select without explicit user consent
        let selected = arbitrator.selectDevice(uid: builtInMicUID, explicitUserConsent: false)
        guard !selected else {
            print("     [FAIL] Device selection succeeded without user consent!")
            return false
        }

        guard arbitrator.currentState != .active else {
            print("     [FAIL] Subsystem entered active state without user consent")
            return false
        }

        print("     [PASS] Unauthorized device fallback cleanly rejected.")
        return true
    }

    /// Verifies that re-plugging or re-selecting a device with explicit consent restores active audio.
    public static func testReconnectionWithConsentRestoresStreaming() -> Bool {
        print("  -> testReconnectionWithConsentRestoresStreaming")
        let arbitrator = HostDeviceArbitrator()
        let usbUID = "com.clearcore.audio.usb.mic-02"

        arbitrator.registerDevice(AudioDeviceDescription(
            uid: usbUID,
            name: "Podcast USB Mic",
            isBuiltIn: false,
            isConnected: true
        ))

        _ = arbitrator.selectDevice(uid: usbUID, explicitUserConsent: true)
        arbitrator.handleDeviceDisconnection(uid: usbUID)
        guard arbitrator.currentState == .disconnectedWaitingForConsent else {
            return false
        }

        // Reconnect device and re-select with explicit user consent
        arbitrator.registerDevice(AudioDeviceDescription(
            uid: usbUID,
            name: "Podcast USB Mic",
            isBuiltIn: false,
            isConnected: true
        ))

        let reselected = arbitrator.selectDevice(uid: usbUID, explicitUserConsent: true)
        guard reselected, arbitrator.currentState == .active else {
            print("     [FAIL] Failed to restore streaming after reconnection with consent")
            return false
        }

        let frameCount = 480
        let audioBuffer = UnsafeMutablePointer<Float>.allocate(capacity: frameCount)
        defer { audioBuffer.deallocate() }

        arbitrator.renderAudio(into: audioBuffer, frameCount: frameCount)
        guard audioBuffer.pointee == 0.42 else {
            print("     [FAIL] Did not resume active audio rendering after reconnection")
            return false
        }

        print("     [PASS] Reconnection with explicit user consent correctly resumes streaming.")
        return true
    }
}

// Direct executable invocation entry point
#if !TEST_HOST_STANDALONE
let status = HotplugIntegrationTests.runAll()
if !status {
    exit(1)
}
#endif
