# Implementation Plan: Task 11 Production Windows WASAPI & WaveRT Host Adapter

## Phase 1: Workspace Registration & RED Tests
- [x] Task: Create `platform/windows/host/Cargo.toml` and register in root `Cargo.toml`
    - [x] Create `platform/windows/host/Cargo.toml`
    - [x] Register `platform/windows/host` in workspace `members`
- [x] Task: Write failing integration tests (RED)
    - [x] Write `platform/windows/host/tests/hotplug.rs` (`selected_device_loss_enters_waiting_without_selecting_another_mic`)
    - [x] Write `platform/windows/host/tests/session.rs` (session contention and ownership rejection)
    - [x] Write `platform/windows/host/tests/endpoint_formats.rs` (mono 48kHz PCM16 and Float32 conversion)
    - [x] Verify tests fail cleanly (RED) in Docker

## Phase 2: Implementation of WASAPI Backend & WaveRT Endpoint (GREEN)
- [x] Task: Implement endpoint types and contracts
    - [x] Create `platform/windows/host/src/lib.rs`
    - [x] Implement `platform/windows/host/src/wasapi.rs` (`WasapiAudioBackend`, device discovery, hotplug handler)
    - [x] Implement `platform/windows/host/src/endpoint.rs` (`WindowsVirtualMicrophone`, IOCTL sender, silence policy)
    - [x] Verify unit and integration tests pass (GREEN) in Docker

## Phase 3: Driver Polish & Evidence Documentation
- [x] Task: Update driver sources and create integration evidence
    - [x] Polish `platform/windows/driver/src/WaveRtMiniport.cpp` and `IoctlTransport.cpp`
    - [x] Create `docs/evidence/windows-integration.md` documenting WASAPI capture, WaveRT submission, and application compatibility
- [x] Task: Verify workspace integrity and complete track
