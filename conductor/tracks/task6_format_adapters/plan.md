# Implementation Plan: Task 6 Framing, Resampling & External Format Adapters

## Phase 1: Workspace Registration & Contract Tests (RED) [checkpoint: 83efb79]
- [x] Task: Create `crates/format-adapter` crate and register in workspace
    - [x] Create `crates/format-adapter/Cargo.toml`
    - [x] Add `crates/format-adapter` to workspace members in root `Cargo.toml`
- [x] Task: Write initial failing contract tests (RED)
    - [x] Write `crates/format-adapter/tests/framing.rs` with test `accumulator_emits_one_480_sample_frame_from_irregular_callbacks`
    - [x] Write `crates/format-adapter/tests/formats.rs` with test `unsupported_endpoint_format_fails_instead_of_converting_implicitly`
    - [x] Verify tests fail cleanly (RED) in Docker
- [x] Task: Phase Verification & Checkpoint (Refer to workflow.md)

## Phase 2: Accumulator, Deframer & Format Converter (GREEN) [checkpoint: fbde482]
- [x] Task: Implement framing and conversion core
    - [x] Implement `crates/format-adapter/src/input_accumulator.rs`
    - [x] Implement `crates/format-adapter/src/output_deframer.rs`
    - [x] Implement `crates/format-adapter/src/endpoint_converter.rs`
    - [x] Implement `crates/format-adapter/src/resampler.rs`
    - [x] Export public types in `crates/format-adapter/src/lib.rs`
- [x] Task: Verify Phase 1 contract tests pass (GREEN)
    - [x] Run `cargo test -p realtime-noise-format-adapter --locked --offline` in Docker
- [x] Task: Phase Verification & Checkpoint (Refer to workflow.md)

## Phase 3: Adapter Worker & Multi-Format Stress Tests (GREEN & STRESS) [checkpoint: 434efcb]
- [x] Task: Implement format adapter worker and comprehensive tests
    - [x] Implement `crates/format-adapter/src/format_worker.rs`
    - [x] Write `crates/format-adapter/tests/resampling.rs` for sample rate validation
    - [x] Run full format-adapter test suite and clippy in Docker
- [x] Task: Phase Verification & Checkpoint (Refer to workflow.md)

## Phase 4: Final Verification, Checkpoint & Track Completion [checkpoint: 434efcb]
- [x] Task: Full offline workspace verification
    - [x] Run all workspace tests in Docker: `cargo test --workspace --features tract --locked --offline`
    - [x] Ensure formatting `cargo fmt --all -- --check` and strict Clippy across all crates
- [x] Task: Update ledger and mark track complete
    - [x] Append Task 6 completion event to `.omo/start-work/ledger.jsonl`
    - [x] Mark track `[x]` in `conductor/tracks.md` and commit
