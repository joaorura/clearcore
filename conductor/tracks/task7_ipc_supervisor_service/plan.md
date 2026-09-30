# Implementation Plan: Task 7 Local IPC, EngineSupervisor & Service Lifecycle

## Phase 1: Workspace Registration & Contract Tests (RED)
- [x] Task: Create `crates/ipc`, `crates/supervisor`, `crates/service` and register in root `Cargo.toml`
    - [x] Create `crates/ipc/Cargo.toml`
    - [x] Create `crates/supervisor/Cargo.toml`
    - [x] Create `crates/service/Cargo.toml`
    - [x] Register all 3 crates in root `Cargo.toml` `members`
- [x] Task: Write initial failing contract tests (RED)
    - [x] Write `crates/ipc/tests/protocol.rs` with `incompatible_ipc_version_is_rejected_closed`
    - [x] Write `crates/supervisor/tests/supervision.rs` with `sixth_crash_in_fifteen_minutes_enters_terminal_safe_state`
    - [x] Write `crates/service/tests/service_lifecycle.rs` with `service_survives_control_client_disconnect_and_serves_next_client`
    - [x] Verify tests fail cleanly (RED) in Docker
- [ ] Task: Phase Verification & Checkpoint (Refer to workflow.md)

## Phase 2: Protocol, Supervisor & Backoff Implementation (GREEN)
- [ ] Task: Implement IPC Protocol and Transport Core
    - [ ] Implement `crates/ipc/src/protocol.rs` (`realtime-noise.v1`, requests, responses, commands, error codes)
    - [ ] Implement `crates/ipc/src/server.rs` (local stream server / client transport)
    - [ ] Re-export in `crates/ipc/src/lib.rs`
- [ ] Task: Implement Fault-Tolerant EngineSupervisor
    - [ ] Implement `crates/supervisor/src/backoff.rs` (`BACKOFF_SECONDS: [1, 2, 4, 8, 16]`, `MAX_CRASHES_PER_15_MINUTES: 5`)
    - [ ] Implement `crates/supervisor/src/supervisor.rs` (`EngineSupervisor`, `SupervisorStatus`, `TerminalSafeState`)
    - [ ] Re-export in `crates/supervisor/src/lib.rs`
- [ ] Task: Verify Phase 1 IPC and Supervisor tests pass (GREEN)
    - [ ] Run `cargo test -p realtime-noise-ipc -p realtime-noise-supervisor --locked --offline` in Docker
- [ ] Task: Phase Verification & Checkpoint (Refer to workflow.md)

## Phase 3: Daemon Binary, Lifecycle Scripts & Integration Tests (GREEN & INTEGRATION)
- [ ] Task: Implement `realtime-noise-service` daemon
    - [ ] Implement `crates/service/src/bootstrap.rs` (configuration & engine wiring)
    - [ ] Implement `crates/service/src/install.rs` (cross-platform service installer/uninstaller logic)
    - [ ] Implement `crates/service/src/main.rs` (`--run`, `--install-user-service`, `--uninstall-user-service`)
- [ ] Task: Create platform packaging and service descriptors
    - [ ] Create `platform/windows/service/register-user-service.ps1`
    - [ ] Create `platform/windows/service/unregister-user-service.ps1`
    - [ ] Create `packaging/macos/LaunchAgents/com.clearcore.realtime-noise.plist`
    - [ ] Create `packaging/linux/systemd/user/realtime-noise.service`
    - [ ] Create `docs/ipc-v1.md` and `docs/service-lifecycle.md`
- [ ] Task: Verify service lifecycle tests and release build
    - [ ] Run `cargo test -p realtime-noise-service --locked --offline` in Docker
    - [ ] Build release binary: `cargo build -p realtime-noise-service --release --locked --offline` in Docker
- [ ] Task: Phase Verification & Checkpoint (Refer to workflow.md)

## Phase 4: Final Verification, Checkpoint & Track Completion
- [ ] Task: Full offline workspace verification
    - [ ] Run all workspace tests in Docker: `cargo test --workspace --features tract --locked --offline`
    - [ ] Ensure formatting `cargo fmt --all -- --check` and strict Clippy across all crates
- [ ] Task: Update ledger and mark track complete
    - [ ] Append Task 7 completion event to `.omo/start-work/ledger.jsonl`
    - [ ] Mark track `[x]` in `conductor/tracks.md` and commit
