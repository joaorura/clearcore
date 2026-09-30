# Implementation Plan: Task 5 Realtime Noise Engine, Queues, Generations & Silence Policy

## Phase 1: Workspace Registration & Contract Tests (RED)
- [ ] Task: Create `crates/engine` crate and register in workspace
    - [ ] Create `crates/engine/Cargo.toml` with edition 2024 and dependencies on `realtime-noise-contracts`, `realtime-noise-model`
    - [ ] Add `crates/engine` to workspace members in root `Cargo.toml`
    - [ ] Run `cargo check --workspace --locked --offline` in Docker
- [ ] Task: Write initial failing contract tests (RED)
    - [ ] Write `crates/engine/tests/deadline.rs` with test `inference_deadline_miss_closes_generation_and_outputs_silence`
    - [ ] Write `crates/engine/tests/generation.rs` with test `bypass_keeps_framing_without_calling_inference_or_raw_fallback`
    - [ ] Verify that tests fail cleanly with missing types/methods
    - [ ] Commit RED state to Git history with git notes
- [ ] Task: Phase Verification & Checkpoint (Refer to workflow.md)

## Phase 2: Engine Core, Generations & Modes (GREEN)
- [ ] Task: Implement generation lifecycle & engine types
    - [ ] Implement `crates/engine/src/generation.rs` with `Generation`, `GenerationId`, and warm-up state machine
    - [ ] Implement `crates/engine/src/lib.rs` exporting public types
    - [ ] Implement `crates/engine/src/engine.rs` with `DenoiseEngine`, `DenoiseMode`, `EngineStatus`, and `EngineError`
- [ ] Task: Verify Phase 1 tests pass (GREEN)
    - [ ] Run `cargo test -p realtime-noise-engine --locked --offline` in Docker
    - [ ] Commit GREEN state to Git history with git notes
- [ ] Task: Phase Verification & Checkpoint (Refer to workflow.md)

## Phase 3: Bounded Queues, Backpressure & Worker Off-Thread (GREEN & STRESS)
- [ ] Task: Implement bounded queues and backpressure
    - [ ] Implement `crates/engine/src/queue.rs` with 24-hop bounded queue and 20 ms / 2-hop age watermark
    - [ ] Implement `crates/engine/src/worker.rs` with off-thread worker loop and backend switching on hop boundary
    - [ ] Write `crates/engine/tests/backpressure.rs` verifying frame drop, discontinuity flags, and fail-closed silence
- [ ] Task: Run full engine test suite & stress tests in Docker
    - [ ] Run `cargo test -p realtime-noise-engine`
    - [ ] Run `cargo clippy -p realtime-noise-engine -- -D warnings`
    - [ ] Commit implementation cleanly to Git history with git notes
- [ ] Task: Phase Verification & Checkpoint (Refer to workflow.md)

## Phase 4: Final Verification, Checkpoint & Track Completion
- [ ] Task: Run full offline workspace verification in Docker
    - [ ] Run all workspace tests: `cargo test --workspace --features tract --locked --offline`
    - [ ] Ensure formatting `cargo fmt --all -- --check` and strict Clippy across all crates
- [ ] Task: Update ledger and mark track complete
    - [ ] Append Task 5 completion event to `.omo/start-work/ledger.jsonl`
    - [ ] Mark track `[x]` in `conductor/tracks.md` and commit
