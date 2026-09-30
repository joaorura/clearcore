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
    var supervisorStateChanges: [SupervisorState] = []
    var generationChanges: [UInt64] = []
    var errors: [XpcError] = []

    func bridge(_ bridge: EngineXpcClient, didChangeState newState: BridgeState) {
        stateChanges.append(newState)
    }

    func bridge(_ bridge: EngineXpcClient, didUpdateSupervisorState supervisorState: SupervisorState) {
        supervisorStateChanges.append(supervisorState)
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

    func testConflictingSessionReceivesUnavailableBusy() {
        testOwnerSessionLockRejectsConflictingSession()
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

    func testSupervisorStatesUpdateHalModeAndOwnerLock() {
        let client = EngineXpcClient()
        let delegate = MockBridgeDelegate()
        client.delegate = delegate

        var observedHalModes: [DenoiseMode] = []
        var observedLockStates: [(pid_t?, Bool)] = []

        client.onHalModeChange = { mode in
            observedHalModes.append(mode)
        }
        client.onHalOwnerLockChange = { pid, isOwned in
            observedLockStates.append((pid, isOwned))
        }

        let expConnect = expectation(description: "Connect")
        client.connect()
        DispatchQueue.global().asyncAfter(deadline: .now() + 0.05) { expConnect.fulfill() }
        wait(for: [expConnect], timeout: 2.0)

        let expAcquire = expectation(description: "Acquire session")
        client.acquireSession(sessionToken: "sess-engine-1", pid: 9001) { res in
            if case .success = res { expAcquire.fulfill() }
        }
        wait(for: [expAcquire], timeout: 2.0)
        XCTAssertTrue(client.isOwnerSessionActive)
        XCTAssertEqual(client.currentOwnerPID, 9001)

        // 1. Transition to EngineUnavailable: mode becomes mute, generation invalidates
        let genBeforeUnavailable = client.generation
        client.handleSupervisorStateUpdate(.engineUnavailable(reason: "Microphone disconnected"))
        let expUnavailable = expectation(description: "EngineUnavailable")
        DispatchQueue.global().asyncAfter(deadline: .now() + 0.05) { expUnavailable.fulfill() }
        wait(for: [expUnavailable], timeout: 2.0)

        XCTAssertEqual(client.mode, .mute)
        XCTAssertGreaterThan(client.generation, genBeforeUnavailable)

        // 2. Transition to Restarting: mode remains mute, generation invalidates
        let genBeforeRestarting = client.generation
        client.handleSupervisorStateUpdate(.restarting(attempt: 1, nextRetryMs: 1000))
        let expRestarting = expectation(description: "Restarting")
        DispatchQueue.global().asyncAfter(deadline: .now() + 0.05) { expRestarting.fulfill() }
        wait(for: [expRestarting], timeout: 2.0)

        XCTAssertEqual(client.mode, .mute)
        XCTAssertGreaterThan(client.generation, genBeforeRestarting)

        // 3. Transition to TerminalSafeState: mode remains mute, generation invalidates, owner lock released
        let genBeforeTerminal = client.generation
        client.handleSupervisorStateUpdate(.terminalSafeState(reason: "Crash budget exhausted", diagnostic: "5 crashes in 15m"))
        let expTerminal = expectation(description: "TerminalSafeState")
        DispatchQueue.global().asyncAfter(deadline: .now() + 0.05) { expTerminal.fulfill() }
        wait(for: [expTerminal], timeout: 2.0)

        XCTAssertEqual(client.mode, .mute)
        XCTAssertGreaterThan(client.generation, genBeforeTerminal)
        XCTAssertFalse(client.isOwnerSessionActive)
        XCTAssertNil(client.currentOwnerPID)

        // 4. Transition back to Running: mode becomes active
        client.handleSupervisorStateUpdate(.running)
        let expRunning = expectation(description: "Running")
        DispatchQueue.global().asyncAfter(deadline: .now() + 0.05) { expRunning.fulfill() }
        wait(for: [expRunning], timeout: 2.0)

        XCTAssertEqual(client.mode, .active)
    }
}
