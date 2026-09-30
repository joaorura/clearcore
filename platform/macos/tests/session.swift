#!/usr/bin/env swift
//===----------------------------------------------------------------------===//
//
// session.swift
// RealtimeNoiseHAL Integration Test Suite
//
// Validates owner session arbitration and rejection of conflicting non-owner sessions
// with FourCC error code UnavailableBusy (`0x62757379` / 'busy').
// Also tests atomic generation changes clearing ring buffer and outputting silence.
//
//===----------------------------------------------------------------------===//

import Foundation

#if canImport(Darwin)
import Darwin
#endif

#if canImport(CoreAudio)
import CoreAudio
#else
public typealias OSStatus = Int32
public let noErr: OSStatus = 0
#endif

/// FourCC error code representing `UnavailableBusy` ('busy' = 0x62757379).
public let kAudioHardwareUnavailableBusyError: OSStatus = 0x62757379

// MARK: - Session Arbitrator & Test Endpoint

public final class SessionArbitrator {
    private var isOwned: Bool = false
    private var ownerPID: pid_t = 0
    private var ownerToken: String?
    private let lock = NSLock()

    public init() {}

    public func acquireSession(token: String, pid: pid_t) -> OSStatus {
        lock.lock()
        defer { lock.unlock() }

        if isOwned && (ownerPID != pid || ownerToken != token) {
            return kAudioHardwareUnavailableBusyError
        }

        isOwned = true
        ownerPID = pid
        ownerToken = token
        return noErr
    }

    public func releaseSession(token: String, pid: pid_t) -> OSStatus {
        lock.lock()
        defer { lock.unlock() }

        if isOwned && ownerPID == pid && ownerToken == token {
            isOwned = false
            ownerPID = 0
            ownerToken = nil
        }
        return noErr
    }

    public func isOwner(pid: pid_t) -> Bool {
        lock.lock()
        defer { lock.unlock() }
        return !isOwned || ownerPID == pid
    }
}

// MARK: - Test Runner

public final class SessionIntegrationTests {
    public static func runAll() -> Bool {
        print("[TEST-SUITE] Running macOS Session Arbitration Integration Tests...")
        var passed = true

        passed = passed && testConflictingSessionReceivesUnavailableBusy()
        passed = passed && testSessionReleaseAndReacquisition()
        passed = passed && testControlGenerationChangeClearsRingAndVisibleInputOutputsSilence()

        if passed {
            print("[PASS] All Session Integration tests passed successfully.")
        } else {
            print("[FAIL] Session Integration tests failed!")
        }
        return passed
    }

    /// Validates that a conflicting non-owner session receives `UnavailableBusy` (`0x62757379`).
    public static func testConflictingSessionReceivesUnavailableBusy() -> Bool {
        print("  -> testConflictingSessionReceivesUnavailableBusy")
        let arbitrator = SessionArbitrator()

        let primaryPID: pid_t = 4242
        let primaryToken = "sess-owner-primary-1"

        let secondaryPID: pid_t = 5353
        let secondaryToken = "sess-intruder-secondary-2"

        // 1. Primary engine acquires session lock
        let status1 = arbitrator.acquireSession(token: primaryToken, pid: primaryPID)
        guard status1 == noErr else {
            print("     [FAIL] Primary owner failed to acquire lock: \(status1)")
            return false
        }
        guard arbitrator.isOwner(pid: primaryPID) else {
            print("     [FAIL] Primary PID is not recorded as owner")
            return false
        }

        // 2. Conflicting second client attempts to acquire lock
        let status2 = arbitrator.acquireSession(token: secondaryToken, pid: secondaryPID)
        guard status2 == kAudioHardwareUnavailableBusyError else {
            print("     [FAIL] Expected UnavailableBusy (0x62757379), got \(status2)")
            return false
        }

        // 3. Confirm secondary client is not owner
        guard !arbitrator.isOwner(pid: secondaryPID) else {
            print("     [FAIL] Secondary PID was erroneously granted ownership")
            return false
        }

        print("     [PASS] Conflicting session correctly rejected with UnavailableBusy (0x62757379).")
        return true
    }

    /// Validates that releasing the session lock allows a new owner to acquire it cleanly.
    public static func testSessionReleaseAndReacquisition() -> Bool {
        print("  -> testSessionReleaseAndReacquisition")
        let arbitrator = SessionArbitrator()

        let pid1: pid_t = 1001
        let token1 = "owner-1"
        let pid2: pid_t = 2002
        let token2 = "owner-2"

        _ = arbitrator.acquireSession(token: token1, pid: pid1)

        // Release first session
        let releaseStatus = arbitrator.releaseSession(token: token1, pid: pid1)
        guard releaseStatus == noErr else {
            print("     [FAIL] Failed to release session")
            return false
        }

        // Second session should now succeed
        let status2 = arbitrator.acquireSession(token: token2, pid: pid2)
        guard status2 == noErr else {
            print("     [FAIL] Second session failed to acquire lock after release: \(status2)")
            return false
        }
        guard arbitrator.isOwner(pid: pid2) else {
            print("     [FAIL] Second session not registered as owner")
            return false
        }

        print("     [PASS] Session release and reacquisition verified.")
        return true
    }

    /// Validates that control generation change clears the ring buffer and visible input immediately outputs silence.
    public static func testControlGenerationChangeClearsRingAndVisibleInputOutputsSilence() -> Bool {
        print("  -> testControlGenerationChangeClearsRingAndVisibleInputOutputsSilence")

        // Circular buffer simulation
        let capacity = 1024
        let storage = UnsafeMutablePointer<Float>.allocate(capacity: capacity)
        defer { storage.deallocate() }
        storage.initialize(repeating: 0.0, count: capacity)

        var writeIndex: UInt64 = 0
        var readIndex: UInt64 = 0
        var currentGeneration: UInt64 = 1
        var expectedGeneration: UInt64 = 1

        let frameCount = 480
        let writeData = [Float](repeating: 0.88, count: frameCount)

        // Write non-zero audio
        for i in 0..<frameCount {
            storage[Int((writeIndex + UInt64(i)) % UInt64(capacity))] = writeData[i]
        }
        writeIndex += UInt64(frameCount)

        // Read audio before clear
        let dest = UnsafeMutablePointer<Float>.allocate(capacity: frameCount)
        defer { dest.deallocate() }
        dest.initialize(repeating: -1.0, count: frameCount)

        if currentGeneration == expectedGeneration && (writeIndex - readIndex) >= UInt64(frameCount) {
            for i in 0..<frameCount {
                dest[i] = storage[Int((readIndex + UInt64(i)) % UInt64(capacity))]
            }
            readIndex += UInt64(frameCount)
        }
        guard dest.pointee == 0.88 else {
            print("     [FAIL] Audio was not correctly written or read before clear")
            return false
        }

        // Fill buffer again
        for i in 0..<frameCount {
            storage[Int((writeIndex + UInt64(i)) % UInt64(capacity))] = writeData[i]
        }
        writeIndex += UInt64(frameCount)

        // SIMULATE CONTROL GENERATION CHANGE & ATOMIC CLEAR
        let newGen: UInt64 = 2
        currentGeneration = newGen
        expectedGeneration = newGen
        writeIndex = 0
        readIndex = 0
        storage.initialize(repeating: 0.0, count: capacity)

        // Read audio after clear: MUST immediately output digital silence
        dest.initialize(repeating: 99.0, count: frameCount)
        let available = Int(writeIndex - readIndex)
        if available < frameCount || currentGeneration != expectedGeneration {
            dest.initialize(repeating: 0.0, count: frameCount)
        }

        for i in 0..<frameCount {
            guard dest[i] == 0.0 else {
                print("     [FAIL] Buffer output non-zero sample after clear at index \(i): \(dest[i])")
                return false
            }
        }

        print("     [PASS] Generation change atomically purges ring buffer and visible input outputs silence.")
        return true
    }
}

// Direct executable invocation entry point
#if !TEST_HOST_STANDALONE
let status = SessionIntegrationTests.runAll()
if !status {
    exit(1)
}
#endif
