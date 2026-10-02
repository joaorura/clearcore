# Specification: Task 7 Local IPC, EngineSupervisor & Service Lifecycle

**Track ID:** `task7_ipc_supervisor_service`  
**Created:** 2026-09-30  
**Phase:** Wave 2 (Onda 2)  
**Parent Plan:** [`docs/superpowers/plans/2026-09-23-realtime-noise-suppression-plan.md`](../../../docs/superpowers/plans/2026-09-23-realtime-noise-suppression-plan.md#L375-L422)

---

## 1. Overview & Objective

Implement local control-plane IPC (`crates/ipc`), fault-tolerant supervision with bounded exponential backoff (`crates/supervisor`), and the standalone user-level daemon (`crates/service`, executable `realtime-noise-service`) with cross-platform service lifecycle scripts (Windows logon task, macOS LaunchAgent, Linux systemd --user).

---

## 2. Architecture & Contracts

1. **Protocol (`realtime-noise.v1`):**
   - Strictly versioned local IPC over domain sockets (Unix) / named pipes (Windows).
   - Envelope: `{ version: "realtime-noise.v1", request_id: String, command: Command, payload: Value }`.
   - Control plane only: **Zero PCM audio transported over IPC**.
   - Fail-closed: Incompatible version requests are rejected with `VersionMismatch`.
   - Safe client disconnect: Daemon outlives UI disconnects and accepts subsequent client connections.

2. **Fault-Tolerant EngineSupervisor:**
   - Supervises `DenoiseEngine` worker processes/threads.
   - States: `Running`, `EngineUnavailable`, `Restarting { attempt: usize, next_retry_ms: u64 }`, `TerminalSafeState { reason: String }`.
   - Normative backoff: `const BACKOFF_SECONDS: [u64; 5] = [1, 2, 4, 8, 16];`
   - Crash limit: `const MAX_CRASHES_PER_15_MINUTES: usize = 5;`
   - On 6th crash within 15 minutes, transitions into `TerminalSafeState`:
     - Maintains digital silence on endpoint.
     - Preserves diagnostic error logs.
     - Disallows automatic restart; requires explicit user action via IPC.

3. **Daemon Service (`realtime-noise-service`):**
   - CLI commands:
     - `--run`: Continuous supervisor and IPC listener loop.
     - `--install-user-service`: Installs per-user service unit.
     - `--uninstall-user-service`: Uninstalls per-user service unit.
   - Per-user ownership and session separation (Windows scheduled task on logon, macOS LaunchAgent, Linux systemd `--user`).

4. **Documentation & Packaging:**
   - `docs/ipc-v1.md`: Specification of messages, status codes, and security boundary.
   - `docs/service-lifecycle.md`: Supervised lifecycle, states, backoff, and recovery.
   - `platform/windows/service/{register-user-service.ps1,unregister-user-service.ps1}`
   - `packaging/macos/LaunchAgents/com.clearcore.realtime-noise.plist`
   - `packaging/linux/systemd/user/realtime-noise.service`

---

## 3. Non-Functional Requirements

- `#![forbid(unsafe_code)]` across all crates.
- Zero allocation in audio paths; control plane separated from DSP.
- 100% offline Docker test verification with zero warnings under `--features tract --locked --offline`.
