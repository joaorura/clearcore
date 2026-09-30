# Implementation Plan: Task 15 Accelerator Plugins & AUTO Policy

## Phase 1: Workspace Registration & RED Tests
- [x] Task: Create `crates/accelerators/Cargo.toml` and register in root `Cargo.toml`
- [x] Task: Write failing test `auto_uses_warmed_tract_when_plugin_fails_quality_gate`
- [x] Task: Verify RED phase in Docker

## Phase 2: Accelerator Backends & AUTO Policy Implementation (GREEN)
- [x] Task: Implement `crates/accelerators/src/lib.rs`
- [x] Task: Implement `crates/accelerators/src/openvino.rs`
- [x] Task: Implement `crates/accelerators/src/cuda.rs`
- [x] Task: Implement `crates/accelerators/src/coreml.rs`
- [x] Task: Implement `crates/accelerators/src/auto.rs`
- [x] Task: Implement `crates/tools/src/bin/calibrate-backend.rs`
- [x] Task: Verify tests pass (GREEN) in Docker

## Phase 3: Hardware Gate & Qualification Document
- [x] Task: Generate `docs/accelerator-qualification.md` documenting NVIDIA RTX PRO 1000 Blackwell (sm_120) and Intel NPU status
- [x] Task: Verify workspace integrity and complete track
