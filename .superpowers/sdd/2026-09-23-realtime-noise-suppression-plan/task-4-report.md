# Task 4 Report: Gated CPU Reference Backend and Golden Harness

## Status

Task 4 software remediation is implemented, but M1 is not approved. The current durable status is `BLOCKED_PENDING_GOLDEN`; no validated corpus or frozen golden exists. Physical qualification also remains blocked by the unavailable reference-class host and per-hop allocation instrumentation.

## Implemented

- The Rust M0 adapter now validates exact record schemas and cross-record legal, license, conversion, key, signature, archive, and fixed-policy bindings.
- `TractBackend::new` reruns M0 verification and consumes the new verified archive snapshot before model parsing.
- The real tract smoke test drives twenty non-silent hops and observes stateful, finite, non-zero output.
- The strict golden schema binds complete M0/backend/corpus/generator/host/quality provenance, ordered input and output frames, checksums, exact frame counts, and bounded numerical tolerances.
- `generate-golden --fresh` validates an approved local corpus manifest, isolated-host environment, frame checksums, overwrite provenance identity, real backend output, and writes atomically.
- `benchmark` validates its exact command, M0, golden descriptor, host qualification, warm-up workload, and real 300-second per-hop timings. It records p50/p95/p99/max and deadline misses in canonical JSON. It cannot approve M1 while allocation instrumentation is unavailable under the repository's `unsafe_code = forbid` policy.
- The offline gate compiles and tests the `tract` feature instead of silently testing only the default-empty model crate.

## Evidence

- Genuine fix-round RED: `.superpowers/sdd/2026-09-23-realtime-noise-suppression-plan/task-4-evidence/fix-round-1-red.txt`.
- The original root `task-4-evidence/` files were removed because they were outside the Task 4 evidence location and the claimed RED was a green run.
- `docs/evidence/m0-asset-gate.json` was restored byte-for-byte to the pre-Task-4 revision; fresh verification is rerun without committing timestamp churn.

## Remaining Physical Blocks

1. `BLOCKED_PENDING_GOLDEN`: `fixtures/corpus/corpus-manifest.json` and `fixtures/golden/frozen-reference.json` are absent.
2. `BLOCKED_UNSUPPORTED_CPU_PROFILE`: this environment cannot prove the required physical four-core Intel Core i5-10210U-class AVX2 host, sustained frequency, AC power, and topology.
3. `BLOCKED_ALLOCATION_MEASUREMENT`: no approved offline-safe allocation counter is available. No zero-allocation claim is made.
4. No 300-second qualifying benchmark has run because the earlier gates are blocked.

These blocks are not waivers. `M1_APPROVED` may be recorded only after a designated isolated generation host creates and validates the frozen golden and a qualifying physical host completes the benchmark with allocation measurements.
