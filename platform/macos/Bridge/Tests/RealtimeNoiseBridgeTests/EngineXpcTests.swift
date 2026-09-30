//===----------------------------------------------------------------------===//
//
// EngineXpcTests.swift
// RealtimeNoiseBridgeTests
//
// Unit tests for EngineXpcClient out-of-callback bridge and session lock.
//
//===----------------------------------------------------------------------===//

import XCTest
@testable import RealtimeNoiseBridge

final class MockBridgeDelegate: RealtimeNoiseBridgeDelegate, @unchecked Sendable {
    var stateChanges: [BridgeState] = []
    var generationChanges: [UInt64] = []
    var errors: [XpcError] = []

    func bridge(_ bridge: EngineXpcClient, didChangeState newState: BridgeState) {
        stateChanges.append(newState)
    }

    func bridge(_ bridge: EngineXpcClient, didInvalidateGeneration newGeneration: UInt64) {
        generationChanges.append(newGeneration)
    }

    func bridge(_ bridge: EngineXpcClient, didEncounterError error: XpcError) {
        errors.append(error)
    }
}

final class EngineXpcTests: XCTestCase {
    func testOwnerSessionLockRejectsConflictingSession() {
        let client = EngineXpcClient()
        let exp1 = expectation(description: "First session acquires lock")
        let exp2 = expectation(description: "Second conflicting session rejected with UnavailableBusy")

        client.acquireSession(sessionToken: "token-owner-primary", pid: 1234) { result in
            switch result {
            case .success:
                exp1.fulfill()
            case .failure(let err):
                XCTFail("Unexpected failure: \(err)")
            }
        }

        wait(for: [exp1], timeout: 2.0)

        // Attempting to acquire lock with different token
        client.acquireSession(sessionToken: "token-intruder", pid: 5678) { result in
            switch result {
            case .success:
                XCTFail("Conflicting session should not have succeeded")
            case .failure(let err):
                switch err {
                case .unavailableBusy:
                    exp2.fulfill()
                default:
                    XCTFail("Expected unavailableBusy error, got \(err)")
                }
            }
        }

        wait(for: [exp2], timeout: 2.0)
    }

    func testTerminalSafeStateInvalidatesGeneration() {
        let client = EngineXpcClient()
        let delegate = MockBridgeDelegate()
        client.delegate = delegate

        let exp = expectation(description: "Connect")
        client.connect()
        DispatchQueue.global().asyncAfter(deadline: .now() + 0.1) {
            exp.fulfill()
        }
        wait(for: [exp], timeout: 2.0)

        let initialGen = client.generation
        XCTAssertGreaterThan(initialGen, 0)

        // Enter terminal safe state
        let expFailClosed = expectation(description: "Terminal safe state")
        client.enterTerminalSafeState(reason: "Supervisor detected deadline overrun")
        DispatchQueue.global().asyncAfter(deadline: .now() + 0.1) {
            expFailClosed.fulfill()
        }
        wait(for: [expFailClosed], timeout: 2.0)

        XCTAssertGreaterThan(client.generation, initialGen, "Generation must be incremented on fail-closed invalidation")
        if case .terminalSafeState = client.currentState {
            // expected
        } else {
            XCTFail("Current state must be terminalSafeState")
        }
    }
}
