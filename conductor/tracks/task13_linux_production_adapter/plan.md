# Implementation Plan: Task 13 Production Linux PipeWire Host Adapter

## Phase 1: Workspace Registration & RED Tests
- [x] Task: Create `platform/linux/host/Cargo.toml` and register in root `Cargo.toml`
    - [x] Create `platform/linux/host/Cargo.toml`
    - [x] Register `platform/linux/host` in workspace `members`
- [x] Task: Write failing integration tests (RED)
    - [x] Write `platform/linux/host/tests/rebind.rs` (`recreated_source_requires_rebind_before_audio_is_observed`)
    - [x] Write `platform/linux/host/tests/hotplug.rs` (device disconnection transitions to `WaitingForDevice`)
    - [x] Write `platform/linux/host/tests/session.rs` (device contention returns `UnavailableBusy`)
    - [x] Verify tests fail cleanly (RED) in Docker

## Phase 2: Implementation of PipeWire Host Adapter (GREEN)
- [x] Task: Implement host backend and virtual microphone
    - [x] Create `platform/linux/host/src/lib.rs`
    - [x] Implement `platform/linux/host/src/pipewire.rs` (PipeWire audio capture, hotplug handler)
    - [x] Implement `platform/linux/host/src/endpoint.rs` (`LinuxVirtualMicrophone`, helper supervisor, rebind manager)
    - [x] Verify unit and integration tests pass (GREEN) in Docker

## Phase 3: Helper Polish & Evidence Documentation
- [x] Task: Polish helper sources and create integration evidence
    - [x] Polish `platform/linux/helper/src/{pipewire_helper,transport_bridge,format_converter}.c`
    - [x] Create `docs/evidence/linux-integration.md` documenting PipeWire host, rebind semantics, and application compatibility
- [x] Task: Verify workspace integrity and complete track
