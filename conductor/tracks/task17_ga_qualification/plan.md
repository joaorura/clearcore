# Implementation Plan: Task 17 GA Qualification Matrix

## Phase 1: Qualification Crate & Decision Aggregator (RED)
- [x] Task: Write failing test `one_platform_failure_blocks_simultaneous_ga` in `crates/qualification/tests/ga_decision.rs`
- [x] Task: Verify failing test (RED) in Docker

## Phase 2: Decision Aggregator Implementation (GREEN)
- [x] Task: Implement `GaDecision`, `PlatformStatus`, `aggregate` in `crates/qualification/src/lib.rs` (or `decision.rs`)
- [x] Task: Verify tests pass (GREEN) in Docker

## Phase 3: Matrix Automation & Tooling
- [x] Task: Implement `tools/qualification/collect_metrics.rs`
- [x] Task: Implement `tools/qualification/run_matrix.py`
- [x] Task: Run qualification matrix harness across configured targets

## Phase 4: Platform Evidence & Final Decision
- [x] Task: Generate `docs/evidence/windows.md`, `macos.md`, `ubuntu.md`, `fedora.md`
- [x] Task: Generate `docs/evidence/ga-matrix.md`
- [x] Task: Generate `docs/ga-decision.md`
- [x] Task: Verify workspace integrity and complete track
