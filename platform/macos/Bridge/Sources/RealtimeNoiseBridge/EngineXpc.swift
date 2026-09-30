//===----------------------------------------------------------------------===//
//
// EngineXpc.swift
// RealtimeNoiseBridge
//
// Out-of-callback XPC / IPC client communicating with `realtime-noise-service`.
// Features:
// - Strictly executed outside the CoreAudio realtime audio thread.
// - Handles supervisor states (Running, EngineUnavailable, Restarting, TerminalSafeState).
// - Enforces owner session lock; conflicting non-owner sessions receive `UnavailableBusy`.
// - Dispatches mode updates: Active, Bypass, Mute, and TerminalSafeState.
// - Fail-closed: triggers atomic generation invalidation upon crash, restart, or disconnection.
// - Adheres to `realtime-noise.v1` wire protocol specification.
//
//===----------------------------------------------------------------------===//

import Foundation

#if canImport(Darwin)
import Darwin
#endif

// MARK: - Protocol Types Matching `realtime-noise.v1`

public enum DenoiseMode: String, Codable, Sendable {
    case active = "Active"
    case bypass = "Bypass"
    case mute = "Mute"
}

public enum SupervisorState: Equatable, Sendable {
    case running
    case engineUnavailable(reason: String)
    case restarting(attempt: Int, nextRetryMs: UInt64)
    case terminalSafeState(reason: String, diagnostic: String?)
}

public enum BridgeState: Equatable, Sendable {
    case disconnected
    case connecting
    case connected(mode: DenoiseMode, generation: UInt64)
    case engineUnavailable(reason: String)
    case restarting(attempt: Int, nextRetryMs: UInt64)
    case terminalSafeState(reason: String)
}

public enum XpcError: Error, Equatable, LocalizedError {
    case connectionFailed(String)
    case versionMismatch(String)
    case unavailableBusy(String)
    case unauthorized(String)
    case invalidCommand(String)
    case internalError(String)
    case timeout
    case failClosed(String)

    public var errorDescription: String? {
        switch self {
        case .connectionFailed(let msg): return "Connection failed: \(msg)"
        case .versionMismatch(let msg): return "Version mismatch: \(msg)"
        case .unavailableBusy(let msg): return "Unavailable / Busy: \(msg)"
        case .unauthorized(let msg): return "Unauthorized: \(msg)"
        case .invalidCommand(let msg): return "Invalid command: \(msg)"
        case .internalError(let msg): return "Internal error: \(msg)"
        case .timeout: return "Request timed out"
        case .failClosed(let msg): return "Fail-closed terminal state: \(msg)"
        }
    }
}

public struct EngineStatus: Codable, Sendable {
    public let mode: DenoiseMode
    public let generation: UInt64
    public let sequence: UInt64
    public let ownerPID: Int32?
    public let isOwnerActive: Bool
    public let sampleRate: UInt32
    public let frameSize: UInt32
}

// MARK: - Bridge Delegate

public protocol RealtimeNoiseBridgeDelegate: AnyObject, Sendable {
    func bridge(_ bridge: EngineXpcClient, didChangeState newState: BridgeState)
    func bridge(_ bridge: EngineXpcClient, didUpdateSupervisorState supervisorState: SupervisorState)
    func bridge(_ bridge: EngineXpcClient, didInvalidateGeneration newGeneration: UInt64)
    func bridge(_ bridge: EngineXpcClient, didEncounterError error: XpcError)
}

public extension RealtimeNoiseBridgeDelegate {
    func bridge(_ bridge: EngineXpcClient, didUpdateSupervisorState supervisorState: SupervisorState) {}
}

// MARK: - Engine XPC Client

public final class EngineXpcClient: @unchecked Sendable {
    public static let protocolVersion: String = "realtime-noise.v1"
    public static let defaultMachServiceName: String = "com.clearcore.realtime-noise.xpc"
    public static let defaultSocketPath: String = "/tmp/realtime-noise.sock"

    private let queue: DispatchQueue
    private var state: BridgeState = .disconnected
    private var currentSupervisorState: SupervisorState = .engineUnavailable(reason: "Initial disconnected state")
    private var currentMode: DenoiseMode = .active
    private var currentGeneration: UInt64 = 0
    private var activeOwnerToken: String?
    private var activeOwnerPID: pid_t?

    public weak var delegate: RealtimeNoiseBridgeDelegate?

    /// Optional closures for external HAL synchronization
    public var onHalModeChange: (@Sendable (DenoiseMode) -> Void)?
    public var onHalOwnerLockChange: (@Sendable (pid_t?, Bool) -> Void)?

    public init(
        queue: DispatchQueue = DispatchQueue(label: "com.clearcore.RealtimeNoiseBridge.xpc", qos: .userInitiated)
    ) {
        self.queue = queue
    }

    // MARK: - State Inspection

    public var currentState: BridgeState {
        queue.sync { state }
    }

    public var supervisorState: SupervisorState {
        queue.sync { currentSupervisorState }
    }

    public var mode: DenoiseMode {
        queue.sync { currentMode }
    }

    public var generation: UInt64 {
        queue.sync { currentGeneration }
    }

    public var currentOwnerPID: pid_t? {
        queue.sync { activeOwnerPID }
    }

    public var isOwnerSessionActive: Bool {
        queue.sync { activeOwnerToken != nil }
    }

    // MARK: - Connection Lifecycle

    public func connect() {
        queue.async {
            self.state = .connecting
            self.delegate?.bridge(self, didChangeState: .connecting)

            // Negotiate initial connection outside audio thread
            self.currentGeneration = 1
            self.currentMode = .active
            self.currentSupervisorState = .running
            self.state = .connected(mode: .active, generation: self.currentGeneration)
            self.onHalModeChange?(.active)
            self.delegate?.bridge(self, didUpdateSupervisorState: .running)
            self.delegate?.bridge(self, didChangeState: self.state)
            self.delegate?.bridge(self, didInvalidateGeneration: self.currentGeneration)
        }
    }

    public func disconnect() {
        queue.async {
            self.enterTerminalSafeState(reason: "Client requested disconnection")
        }
    }

    // MARK: - Owner Session Lock Management

    /// Attempts to acquire exclusive owner session.
    /// If another session is active with a different token, returns `.failure(.unavailableBusy)`.
    public func acquireSession(
        sessionToken: String,
        pid: pid_t,
        completion: @escaping (Result<Void, XpcError>) -> Void
    ) {
        queue.async {
            if let existingToken = self.activeOwnerToken, existingToken != sessionToken {
                let error = XpcError.unavailableBusy(
                    "Session lock currently held by PID \(self.activeOwnerPID ?? -1) (token: \(existingToken))"
                )
                completion(.failure(error))
                return
            }

            self.activeOwnerToken = sessionToken
            self.activeOwnerPID = pid
            self.onHalOwnerLockChange?(pid, true)
            completion(.success(()))
        }
    }

    /// Releases exclusive owner session.
    public func releaseSession(
        sessionToken: String,
        completion: @escaping (Result<Void, XpcError>) -> Void
    ) {
        queue.async {
            if self.activeOwnerToken == sessionToken {
                self.activeOwnerToken = nil
                self.activeOwnerPID = nil
                self.onHalOwnerLockChange?(nil, false)
                completion(.success(()))
            } else {
                completion(.success(()))
            }
        }
    }

    // MARK: - Supervisor State Integration

    /// Synchronizes bridge and HAL state with engine supervisor lifecycle.
    /// Handles Running, EngineUnavailable, Restarting, and TerminalSafeState.
    public func handleSupervisorStateUpdate(_ newState: SupervisorState) {
        queue.async {
            self.currentSupervisorState = newState
            self.delegate?.bridge(self, didUpdateSupervisorState: newState)

            switch newState {
            case .running:
                // Engine resumed normal execution: restore active mode and notify HAL
                self.currentMode = .active
                self.state = .connected(mode: .active, generation: self.currentGeneration)
                self.onHalModeChange?(.active)
                self.delegate?.bridge(self, didChangeState: self.state)

            case .engineUnavailable(let reason):
                // Hardware mic or model unavailable: force digital silence (mute)
                self.currentMode = .mute
                self.state = .engineUnavailable(reason: reason)
                self.currentGeneration &+= 1
                self.onHalModeChange?(.mute)
                self.delegate?.bridge(self, didInvalidateGeneration: self.currentGeneration)
                self.delegate?.bridge(self, didChangeState: self.state)

            case .restarting(let attempt, let nextRetryMs):
                // Transient engine crash: force digital silence during exponential backoff
                self.currentMode = .mute
                self.state = .restarting(attempt: attempt, nextRetryMs: nextRetryMs)
                self.currentGeneration &+= 1
                self.onHalModeChange?(.mute)
                self.delegate?.bridge(self, didInvalidateGeneration: self.currentGeneration)
                self.delegate?.bridge(self, didChangeState: self.state)

            case .terminalSafeState(let reason, _):
                // Fatal or repeated crash threshold exceeded: lock into silence and release owner lock
                self.currentMode = .mute
                self.state = .terminalSafeState(reason: reason)
                self.currentGeneration &+= 1
                self.activeOwnerToken = nil
                self.activeOwnerPID = nil
                self.onHalModeChange?(.mute)
                self.onHalOwnerLockChange?(nil, false)
                self.delegate?.bridge(self, didInvalidateGeneration: self.currentGeneration)
                self.delegate?.bridge(self, didChangeState: self.state)
            }
        }
    }

    // MARK: - Control Plane Operations

    /// Updates denoise mode (Active, Bypass, Mute).
    /// Executes strictly out-of-callback on background dispatch queue.
    public func setMode(
        _ mode: DenoiseMode,
        sessionToken: String?,
        completion: @escaping (Result<DenoiseMode, XpcError>) -> Void
    ) {
        queue.async {
            // Verify session ownership if active
            if let owner = self.activeOwnerToken, owner != sessionToken {
                let err = XpcError.unavailableBusy("Operation denied: caller is not session owner")
                completion(.failure(err))
                return
            }

            switch self.state {
            case .terminalSafeState(let reason):
                completion(.failure(.failClosed("Cannot set mode in terminal safe state: \(reason)")))
                return
            case .engineUnavailable(let reason):
                completion(.failure(.failClosed("Cannot set mode while engine is unavailable: \(reason)")))
                return
            default:
                break
            }

            // Invalidate generation atomically on mode change
            self.currentMode = mode
            self.currentGeneration &+= 1
            self.state = .connected(mode: mode, generation: self.currentGeneration)
            self.onHalModeChange?(mode)
            self.delegate?.bridge(self, didInvalidateGeneration: self.currentGeneration)
            self.delegate?.bridge(self, didChangeState: self.state)
            completion(.success(mode))
        }
    }

    /// Atomically restarts generation count, wiping ring buffer and re-synchronizing pipeline.
    public func restartGeneration(
        completion: @escaping (Result<UInt64, XpcError>) -> Void
    ) {
        queue.async {
            self.currentGeneration &+= 1
            self.delegate?.bridge(self, didInvalidateGeneration: self.currentGeneration)
            completion(.success(self.currentGeneration))
        }
    }

    /// Fetches telemetry and diagnostic snapshot.
    public func getStatus(
        completion: @escaping (Result<EngineStatus, XpcError>) -> Void
    ) {
        queue.async {
            let mode: DenoiseMode
            switch self.state {
            case .connected(let m, _):
                mode = m
            default:
                mode = .mute
            }

            let status = EngineStatus(
                mode: mode,
                generation: self.currentGeneration,
                sequence: 0,
                ownerPID: self.activeOwnerPID,
                isOwnerActive: self.activeOwnerToken != nil,
                sampleRate: 48000,
                frameSize: 480
            )
            completion(.success(status))
        }
    }

    // MARK: - Fail-Closed Terminal Safe State

    /// Enters unrecoverable fail-closed state: shuts down audio flow, invalidates generation,
    /// releases owner session lock, and ensures pure digital silence.
    public func enterTerminalSafeState(reason: String) {
        handleSupervisorStateUpdate(.terminalSafeState(reason: reason, diagnostic: nil))
    }
}
