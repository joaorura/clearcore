# Implementation Plan: Task 6 Framing, Resampling & External Format Adapters

## Phase 1: Workspace Registration & Contract Tests (RED)
- [ ] Task: Create `crates/format-adapter` crate and register in workspace
    - [ ] Create `crates/format-adapter/Cargo.toml`
    - [ ] Add `crates/format-adapter` to workspace members in root `Cargo.toml`
- [ ] Task: Write initial failing contract tests (RED)
    - [ ] Write `crates/format-adapter/tests/framing.rs` with test `accumulator_emits_one_480_sample_frame_from_irregular_callbacks`
    - [ ] Write `crates/format-adapter/tests/formats.rs` with test `unsupported_endpoint_format_fails_instead_of_converting_implicitly`
    - [ ] Verify tests fail cleanly (RED) in Docker
- [ ] Task: Phase Verification & Checkpoint (Refer to workflow.md)

## Phase 2: Accumulator, Deframer & Format Converter (GREEN)
- [ ] Task: Implement framing and conversion core
    - [ ] Implement `crates/format-adapter/src/input_accumulator.rs`
    - [ ] Implement `crates/format-adapter/src/output_deframer.rs`
    - [ ] Implement `crates/format-adapter/src/endpoint_converter.rs`
    - [ ] Implement `crates/format-adapter/src/resampler.rs`
    - [ ] Export public types in `crates/format-adapter/src/lib.rs`
- [ ] Task: Verify Phase 1 contract tests pass (GREEN)
    - [ ] Run `cargo test -p realtime-noise-format-adapter --locked --offline` in Docker
- [ ] Task: Phase Verification & Checkpoint (Refer to workflow.md)

## Phase 3: Adapter Worker & Multi-Format Stress Tests (GREEN & STRESS)
- [ ] Task: Implement format adapter worker and comprehensive tests
    - [ ] Implement `crates/format-adapter/src/format_worker.rs`
    - [ ] Write `crates/format-adapter/tests/resampling.rs` for sample rate validation
    - [ ] Run full format-adapter test suite and clippy in Docker
- [ ] Task: Phase Verification & Checkpoint (Refer to workflow.md)

## Phase 4: Final Verification, Checkpoint & Track Completion
- [ ] Task: Full offline workspace verification
    - [ ] Run all workspace tests in Docker: `cargo test --workspace --features tract --locked --offline`
    - [ ] Ensure formatting `cargo fmt --all -- --check` and strict Clippy across all crates
- [ ] Task: Update ledger and mark track complete
    - [ ] Append Task 6 completion event to `.omo/start-work/ledger.jsonl`
    - [ ] Mark track `[x]` in `conductor/tracks.md` and commit
