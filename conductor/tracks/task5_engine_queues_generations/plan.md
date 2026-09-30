# Implementation Plan: Task 5 Realtime Noise Engine, Queues, Generations & Silence Policy

## Phase 1: Workspace Registration & Contract Tests (RED) [checkpoint: 4d7a120]
- [x] Task: Create `crates/engine` crate and register in workspace [4d7a120]
    - [x] Create `crates/engine/Cargo.toml` with edition 2024 and dependencies on `realtime-noise-contracts`, `realtime-noise-model`
    - [x] Add `crates/engine` to workspace members in root `Cargo.toml`
    - [x] Run `cargo check --workspace --locked --offline` in Docker
- [x] Task: Write initial failing contract tests (RED) [4d7a120]
    - [x] Write `crates/engine/tests/deadline.rs` with test `inference_deadline_miss_closes_generation_and_outputs_silence`
    - [x] Write `crates/engine/tests/generation.rs` with test `bypass_keeps_framing_without_calling_inference_or_raw_fallback`
    - [x] Verify that tests fail cleanly with missing types/methods
    - [x] Commit RED state to Git history with git notes
- [x] Task: Phase Verification & Checkpoint (Refer to workflow.md) [4d7a120]

## Phase 2: Engine Core, Generations & Modes (GREEN) [checkpoint: 2f38ab1]
- [x] Task: Implement generation lifecycle & engine types [2f38ab1]
    - [x] Implement `crates/engine/src/generation.rs` with `Generation`, `GenerationId`, and warm-up state machine
    - [x] Implement `crates/engine/src/lib.rs` exporting public types
    - [x] Implement `crates/engine/src/engine.rs` with `DenoiseEngine`, `DenoiseMode`, `EngineStatus`, and `EngineError`
- [x] Task: Verify Phase 1 tests pass (GREEN) [2f38ab1]
    - [x] Run `cargo test -p realtime-noise-engine --locked --offline` in Docker
    - [x] Commit GREEN state to Git history with git notes
- [x] Task: Phase Verification & Checkpoint (Refer to workflow.md) [2f38ab1]

## Phase 3: Bounded Queues, Backpressure & Worker Off-Thread (GREEN & STRESS)
- [x] Task: Implement bounded queues and backpressure
    - [x] Implement `crates/engine/src/queue.rs` with 24-hop bounded queue and 20 ms / 2-hop age watermark
    - [x] Implement `crates/engine/src/worker.rs` with off-thread worker loop and backend switching on hop boundary
    - [x] Write `crates/engine/tests/backpressure.rs` verifying frame drop, discontinuity flags, and fail-closed silence
- [x] Task: Run full engine test suite & stress tests in Docker
    - [x] Run `cargo test -p realtime-noise-engine`
    - [x] Run `cargo clippy -p realtime-noise-engine -- -D warnings`
    - [x] Commit implementation cleanly to Git history with git notes
- [x] Task: Phase Verification & Checkpoint (Refer to workflow.md)

## Phase 4: Final Verification, Checkpoint & Track Completion
- [ ] Task: Run full offline workspace verification in Docker
    - [ ] Run all workspace tests: `cargo test --workspace --features tract --locked --offline`
    - [ ] Ensure formatting `cargo fmt --all -- --check` and strict Clippy across all crates
- [ ] Task: Update ledger and mark track complete
    - [ ] Append Task 5 completion event to `.omo/start-work/ledger.jsonl`
    - [ ] Mark track `[x]` in `conductor/tracks.md` and commit
