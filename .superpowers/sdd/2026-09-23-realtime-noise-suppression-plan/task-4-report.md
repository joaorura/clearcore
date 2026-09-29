# Task 4 Report: Gated CPU Reference Backend and Golden Harness

## Status

Task 4 software remediation is implemented, but M1 is not approved. The current canonical receipt status is `BLOCKED_UNSUPPORTED_CPU_PROFILE`; no run-bound qualifying physical-host evidence was supplied. `BLOCKED_PENDING_GOLDEN` remains the next independent gate because no validated corpus or frozen golden exists.

## Implemented

- The Rust M0 adapter now validates exact record schemas and cross-record legal, license, conversion, key, signature, archive, and fixed-policy bindings.
- `TractBackend::new` reruns M0 verification and consumes the new verified archive snapshot before model parsing.
- The real tract smoke test drives twenty non-silent hops and observes stateful, finite, non-zero output.
- The strict golden schema binds complete M0/backend/corpus/generator/host/quality provenance, ordered input and output frames, checksums, exact frame counts, and bounded numerical tolerances.
- `generate-golden --fresh` validates an approved local corpus manifest, isolated-host environment, frame checksums, overwrite provenance identity, real backend output, and writes atomically.
- `benchmark` validates its exact command, M0, golden descriptor, and one ordered pre-measurement output pass against the frozen golden before timing. Its safe `stats_alloc-0.1.10` harness separates initialization, golden verification, warm-up, and measured per-hop allocation counts and bytes, and distinguishes instrumentation availability from a completed measurement. M1 requires zero allocation/reallocation count, a clean worktree, complete host evidence, a measured duration bounded to the requested 300 seconds plus at most one 10 ms deadline, and the latency gates.
- Optional benchmark host evidence is accepted only as the exact suffix `--host-evidence <json> --host-evidence-provenance <record> --run-id <id>`. Schema version 2 replaces the caller-claimed frequency scalar and opaque provenance with strict, byte- and sequence-bounded records. Qualification independently validates exactly 301 ordered snapshots from 0 through 300,000 ms, no gap over 1,000 ms, complete affinity coverage of the locally observed logical-CPU and physical-core topology, finite per-core samples at or above 2,000 MHz, and structured collector/command/timestamp/run provenance whose digest binds the frequency series. Incomplete or duplicate local topology fails closed. The receipt stores full local topology plus only the derived sustained-frequency summary. Missing, oversized, malformed, stale, incomplete, mismatched, symlinked, or non-regular evidence fails closed as `BLOCKED_UNSUPPORTED_CPU_PROFILE` before the golden gate.
- The offline gate compiles and tests the `tract` feature instead of silently testing only the default-empty model crate.

## Evidence

- Genuine fix-round RED: `.superpowers/sdd/2026-09-23-realtime-noise-suppression-plan/task-4-evidence/fix-round-1-red.txt`.
- HOST-001 RED: `.superpowers/sdd/2026-09-23-realtime-noise-suppression-plan/task-4-evidence/host-001-red.txt`.
- HOST-001 GREEN: `.superpowers/sdd/2026-09-23-realtime-noise-suppression-plan/task-4-evidence/host-001-green.txt`.
- HOST-001 manual CLI probes: `.superpowers/sdd/2026-09-23-realtime-noise-suppression-plan/task-4-evidence/host-001-manual.txt`.
- The original root `task-4-evidence/` files were removed because they were outside the Task 4 evidence location and the claimed RED was a green run.
- `docs/evidence/m0-asset-gate.json` was restored byte-for-byte to the pre-Task-4 revision; fresh verification is rerun without committing timestamp churn.

## Verification

- Network-disabled Docker: `cargo fmt --all -- --check`, all-feature workspace build, and all-feature workspace tests passed with `--locked --offline`.
- `scripts/check-offline.sh` passed, including the tract-enabled workspace build and tests; `bash tests/check-offline.sh` passed all eight fail-closed harness scenarios.
- Focused strict Clippy passed for all `realtime-noise-tools` targets, including `host_evidence`, `host_frequency_evidence`, `benchmark`, and `benchmark_receipt`. Full workspace strict Clippy reaches two pre-existing warnings in untouched files: `crates/model/src/archive_adversarial_tests.rs` (`items_after_statements`) and `crates/model/tests/m0_adversarial.rs` (`uninlined_format_args`).
- `validate-asset` returned structured `M0_APPROVED`. `generate-golden --fresh` returned structured `BLOCKED_PENDING_GOLDEN` with exit status 2 because the required isolated-offline generation environment is absent.
- An isolated network-disabled copy with the approved archive removed produced a durable `BLOCKED_NO_APPROVED_ASSET` JSON/Markdown receipt and exit status 2.
- The required benchmark CLI without host evidence produced `BLOCKED_UNSUPPORTED_CPU_PROFILE` and exit status 2. Invalid duration `299` and a partial evidence suffix were rejected with exit status 2; opaque text supplied as both evidence and provenance produced the same safe structured block with null verified evidence and null sustained-frequency summary.
- Native Rust LSP diagnostics were unavailable because this host has no `rust-analyzer`; Docker compilation, tests, and Clippy provide the Rust diagnostics for this change.

## Remaining Physical Blocks

1. `BLOCKED_UNSUPPORTED_CPU_PROFILE`: this environment cannot supply a genuine 300-second per-core frequency observation for the required physical four-core Intel Core i5-10210U-class AVX2 host, AC power, affinity, governor, and topology. The canonical receipt now reports this first unresolved gate.
2. `BLOCKED_PENDING_GOLDEN`: `fixtures/corpus/corpus-manifest.json` and `fixtures/golden/frozen-reference.json` are absent and remain the next gate after host qualification.
3. No physical 300-second qualifying benchmark has run because the earlier gates are blocked. The allocation harness is implemented and available, but has no physical result while golden and host gates prevent the workload. Host qualification fails closed while sustained frequency, AC power, process affinity, and governor evidence remain unobservable.

These blocks are not waivers. `M1_APPROVED` may be recorded only after a designated isolated generation host creates and validates the frozen golden and a qualifying physical host completes the benchmark with allocation measurements.
