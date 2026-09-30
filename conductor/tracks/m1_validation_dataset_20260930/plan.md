# Implementation Plan: M1 Filter Validation Dataset & Task 4 Golden Harness Resolution

## Phase 1: Schema V1/V2 Manifest Consolidation & Verification [checkpoint: 87bfd1b]
- [x] Task: Stage and verify extracted `golden_manifest` module [87bfd1b]
    - [x] Run `cargo test -p realtime-noise-tools --locked --offline` in Docker container
    - [x] Verify Clippy `-D warnings` and formatting on all tools crate targets
    - [x] Commit schema unblock changes cleanly to Git history
- [x] Task: Phase Verification & Checkpoint (Refer to workflow.md) [87bfd1b]

## Phase 2: Speech & Noise Source Lock Binding [checkpoint: 0efca0e]
- [x] Task: Test-first verification of source-lock parser (RED) [49d8119]
    - [x] Write unit test verifying `source_lock_sha256` matching against temp lockfile
- [x] Task: Audio asset inspection & source-lock generation (GREEN) [0efca0e]
    - [x] Acquire and verify EdAcc v1.0 adult speech clips (male and female voices)
    - [x] Verify noise assets (keyboard, fan, traffic, piano, stationary, digital silence, HOMULA-RIR)
    - [x] Generate `fixtures/corpus/source-lock.json` with immutable hashes and licensing metadata
- [x] Task: Phase Verification & Checkpoint (Refer to workflow.md) [0efca0e]

## Phase 3: Audio Normalization & Corpus Manifest V2 [checkpoint: 0135f0d]
- [x] Task: Test frame serialization & validation (RED) [948705c]
    - [x] Write unit tests for 480-sample `f32` normalized frame JSON parser and digest verification
- [x] Task: Audio processing and corpus case generation (GREEN) [0135f0d]
    - [x] Normalize sources via FFmpeg/soxr to 48 kHz mono `f32`
    - [x] Partition into 480-sample hops and emit JSON frame files in `fixtures/corpus/`
    - [x] Generate `fixtures/corpus/corpus-manifest.json` conforming to Schema V2
    - [x] Verify that `validate_corpus` passes completely
- [x] Task: Phase Verification & Checkpoint (Refer to workflow.md) [0135f0d]

## Phase 4: Golden Reference Generation & M1 Qualification
- [ ] Task: Golden reference generation
    - [ ] Execute `generate-golden --fresh` inside offline container
    - [ ] Verify creation and integrity of `fixtures/golden/frozen-reference.json`
- [ ] Task: Latency contract & benchmark qualification
    - [ ] Document 30 ms algorithmic latency amendment in spec and architecture notes
    - [ ] Run benchmark qualification with 300-second sustained frequency observation
    - [ ] Record hardware observation with development host waiver
- [ ] Task: M1 Milestone Approval Audit
    - [ ] Update `task-4-corpus-builder-report.md` and append ruling to `ledger.jsonl`
    - [ ] Verify all gates for `M1_APPROVED` are satisfied
- [ ] Task: Phase Verification & Checkpoint (Refer to workflow.md)
