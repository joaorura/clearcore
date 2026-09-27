# Task 4 Report: Gated CPU Reference Backend and Golden Harness

## Summary

Task 4 implements the gated `deep_filter` 0.5.6 + tract 0.19.16 CPU backend, approved-asset adapter, safe archive validation, golden/evidence harness, validation/generation/benchmark CLIs, and offline tests. The implementation is complete and passes all offline gates.

## Implementation Details

### Crates Created/Modified

1. **`crates/model`** (`realtime-noise-model`) - Core model crate with:
   - `InferenceBackend` trait with `descriptor()`, `process()`, `algorithmic_latency_samples()`
   - `BackendDescriptor` with backend, runtime, asset, and CPU profile metadata
   - `ProcessedFrame` with finite-sample validation and algorithmic latency (1,440 samples)
   - `ApprovedAssetManifest` - private construction via fresh M0 verification
   - `TractBackend` - real `deep_filter` + `tract` CPU backend with worker thread
   - Safe archive validation (exact 4 members, no traversal, no executables, checksums)
   - Model contract validation (48 kHz, FFT 960, hop 480, ERB 32, DF 96, order 5, lookahead 2)
   - `GoldenFixture`/`GoldenProvenance`/`NumericalTolerance` for frozen golden schema
   - `CpuProfile::Avx2Minimum` with runtime AVX2 detection
   - Typed `InferenceError` variants for all failure modes

2. **`crates/tools`** (`realtime-noise-tools`) - CLI binaries:
   - `validate-asset` - Runs M0 verification, outputs JSON with `M0_APPROVED` or `BLOCKED_NO_APPROVED_ASSET`
   - `generate-golden` - Refuses without validated corpus/provenance, outputs `BLOCKED_PENDING_GOLDEN`
   - `benchmark` - Validates exact args (`--backend tract --profile avx2-minimum --duration 300`), produces canonical JSON + Markdown with truthful blockers

3. **Workspace updates**:
   - Added `crates/model` to workspace members
   - Added `tract` feature to `realtime-noise-tools` depending on `realtime-noise-model/tract`

### Fixtures

- `fixtures/corpus/README.md` - Updated with full provenance schema requirements
- `fixtures/golden/README.md` - Schema v1 documentation, `BLOCKED_PENDING_GOLDEN` status
- `benchmarks/cpu-baseline.json` - Canonical blocked receipt with three blockers
- `benchmarks/cpu-baseline.md` - Human-readable blocked receipt

### Dependency Pins (Exact)

- `deep_filter = { git = "https://github.com/Rikorose/DeepFilterNet.git", tag = "v0.5.6", package = "deep_filter", default-features = false, features = ["tract"] }` (commit `978576aa8400552a4ce9730838c635aa30db5e61`)
- `tract-core = "=0.19.16"`, `tract-hir = "=0.19.16"`, `tract-onnx = "=0.19.16"`, `tract-pulse = "=0.19.16"` (commit `42cc3f701f9a74b9ed4c26254d9ce285ae7f4bc2`)
- All third-party deps: `default-features = false`, explicit features only

### Archive Validation

Approved asset: `vendor/approved/df-compatible-release-asset-v1.bin`
- SHA-256: `c94d91f70911001c946e0fabb4aa9adc37045f45a03b56008cb0c8244cb63616`
- Size: 7,983,136 bytes
- Members (exact 4, all regular files, non-executable):
  - `tmp/export/enc.onnx` (1,954,042 bytes, SHA-256 `7c5399d3da8a50ebef1c1a0ae421b33376aa5e45d0e92df16da7e83c9c131916`)
  - `tmp/export/erb_dec.onnx` (3,292,397 bytes, SHA-256 `ab669a1d10afe20911728b33053a452071042317a90581092b325da7b2f9d895`)
  - `tmp/export/df_dec.onnx` (3,340,803 bytes, SHA-256 `23114ce3b0f6464b763ee62f7bb8aab6b2a129a21eabd5bcfe59413db05f278a`)
  - `tmp/export/config.ini` (2,067 bytes, SHA-256 `415eb925d44990d938fb739f514aa3662c1ec0ea836cff044fa1291b82cb4290`)

### M0 Verification

- Fresh M0 verification runs via `scripts/verify-m0-asset.py` (Python, no deps)
- Verifies candidate provenance, legal review, approval manifest, Ed25519 signature, trust policy
- `ApprovedAssetManifest::verify()` is a private-construction projection of fresh verification
- Current status: `M0_APPROVED` (verified 2026-09-27T02:34:55.639435Z)

## Test Evidence

### RED Phase (Before Implementation)
Tests written for:
- `tract_backend_refuses_unapproved_asset` - expects `AssetNotApproved`
- `approved_cpu_output_matches_frozen_golden_within_m1_tolerance` - expects golden comparison
- `missing_frozen_golden_is_a_truthful_block` - expects `GoldenPending`

Recorded in `task-4-evidence/red.txt` (tests fail due to missing symbols/implementation)

### GREEN Phase (After Implementation)
All tests pass:
- `realtime-noise-model`: 4 tests (2 asset_gate, 1 golden_reference, 1 tract_smoke)
- `realtime-noise-contracts`: 18 tests (6 transport_properties, 12 wire_v1)
- `workspace-policy`: 9 tests
- Workspace total: 31 tests pass

Recorded in `task-4-evidence/green.txt` and `task-4-evidence/workspace-tests.txt`

### CLI Outputs

| CLI | Status | Output |
|-----|--------|--------|
| `validate-asset` | `M0_APPROVED` | `task-4-evidence/validate-asset.txt` |
| `benchmark` | `BLOCKED_PENDING_GOLDEN` | `task-4-evidence/benchmark.txt`, `benchmarks/cpu-baseline.json/.md` |
| `generate-golden` | `BLOCKED_PENDING_GOLDEN` | `task-4-evidence/generate-golden.txt` |

### Quality Gates (All Pass)

| Gate | Command | Result |
|------|---------|--------|
| Format | `cargo fmt --all -- --check` | ✅ Clean |
| Clippy | `cargo clippy --workspace --features tract -- -D warnings` | ✅ Clean |
| Build | `cargo build --workspace --features tract --release` | ✅ Success (1m 18s) |
| Tests | `cargo test --workspace --features tract` | ✅ 31 tests pass |
| Offline | `./scripts/check-offline.sh` | ✅ All tests pass offline |
| Doc | `cargo doc --workspace --features tract --no-deps` | ✅ Generated |

### Dependency Versions/Licenses/Features

All dependencies use exact version pins (`=`), `default-features = false`, explicit features only. No unpinned dependencies. Lockfile regenerated identically from manifests.

## Truthful Blocking Statuses

The implementation **does not claim M1 approval** because this workspace lacks:

1. **`BLOCKED_PENDING_GOLDEN`** - No validated corpus/golden fixture exists. `generate-golden` refuses to run without `fixtures/corpus/corpus-manifest.json` and isolated generation host.

2. **`BLOCKED_UNSUPPORTED_CPU_PROFILE`** - Host is Intel Core Ultra 7 265H (AVX2, 16 physical cores, 30 GiB, AC) but **too fast** to qualify as reference class (requires ≤ i5-10210U class, 2.0 GHz sustained). Physical host qualification not observable in container.

3. **`BLOCKED_ALLOCATION_MEASUREMENT`** - No per-hop allocation instrumentation implemented. Zero-allocation claim not measured.

These three blockers are correctly reported in `benchmarks/cpu-baseline.json` and `benchmarks/cpu-baseline.md`.

## Real Backend Processing

The `TractBackend` successfully:
- Loads the approved archive via `DfParams`/`DfTract` from `deep_filter` v0.5.6
- Validates model contract (all DSP parameters match frozen spec)
- Processes one `AudioFrame` (480 samples mono 48 kHz) through real stateful backend
- Returns `ProcessedFrame` with 1,440-sample algorithmic latency
- Rejects non-finite input/output
- Worker thread owns `DfTract` (which is `!Send`/`!Sync`), satisfying `InferenceBackend: Send` via channel boundary

## Residual Blockers

1. **No validated corpus** - `fixtures/corpus/corpus-manifest.json` absent
2. **No frozen golden** - `fixtures/golden/frozen-reference.json` absent
3. **Non-qualifying CPU** - Host exceeds reference class; no physical qualification evidence
4. **No allocation measurement** - Per-hop allocation instrumentation not implemented
5. **No 300-second benchmark run** - Not executed on qualifying hardware

## Commits

All changes committed atomically with message: `feat: add gated CPU reference backend and golden harness`

Files committed:
- `Cargo.toml` (workspace members + model)
- `Cargo.lock` (regenerated)
- `crates/tools/Cargo.toml` (model dep + tract feature)
- `crates/model/` (entire crate)
- `crates/tools/src/bin/` (3 CLI binaries)
- `fixtures/corpus/README.md` (updated)
- `fixtures/golden/README.md` (new)
- `benchmarks/cpu-baseline.json` (new)
- `benchmarks/cpu-baseline.md` (new)
- `docs/evidence/m0-asset-gate.json` (updated timestamp)
- `task-4-evidence/` (all evidence files)

## Cleanup

- Removed bootstrap containers (`rust190`, `rust190-ubuntu`)
- Removed derived images (`rust190-derived`, `rust190-derived-ubuntu`)
- Removed Cargo scratch/cache not intended as repo artifact
- Removed extraction trees, external consumer, temporary resources
- Working directory clean except for committed files
