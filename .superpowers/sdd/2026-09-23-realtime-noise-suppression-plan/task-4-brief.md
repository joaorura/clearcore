# Task 4 Preflight: Gated CPU Reference Backend, Golden Harness, and Benchmark

## Decision

Task 4 may start only from Task 3 HEAD `6ee6df3bfbf4313f07ed213a07e9bda012542ab1` with a clean worktree and an independently re-run M0 gate. Its deliverable is M1 characterization of the only production baseline: `libDF + tract` on CPU. It is not permission to add an accelerator, an engine, a queue, an endpoint adapter, resampling, IPC, fallback behavior, or a network download.

M1 is approved only when all of these are durable and mutually consistent:

- The existing M0 verifier accepts the exact approved archive and a model backend cannot be constructed without that verified approval.
- The corpus provenance and frozen golden fixture are present and valid.
- The tract backend passes deterministic golden comparison and adversarial boundary tests.
- A physical qualifying AVX2 CPU produces a complete 300-second benchmark with measured allocation data, `p99 <= 7 ms`, and no observed invocation above the `10 ms` deadline.

Anything else is a block, not a waiver. Do not fabricate goldens, benchmark evidence, an approval, a CPU profile, allocation data, or a successful tool result.

## Current Feasibility

### Established inputs

- Task 3 provides `realtime_noise_contracts::AudioFrame = [f32; 480]`, mono 48 kHz, and no model crate exists yet.
- The root workspace currently registers `workspace-policy`, `realtime-noise-tools`, `realtime-noise-qualification`, and `realtime-noise-contracts`; `realtime-noise-tools` is an empty registered placeholder. `crates/model/` does not yet exist.
- Cargo is permanently offline (`.cargo/config.toml`); the workspace uses Rust `1.90.0`, edition 2024, resolver 3, workspace lints, and dependencies must explicitly set `default-features = false`.
- `Cargo.lock` presently contains no `tract`, `libDF`, ONNX runtime, or model dependency. Dependency selection and the exact offline-available crate/API surface are unresolved implementation research, not assumptions this brief authorizes.

### Approved model asset

The only allowed archive is the tracked regular file:

`vendor/approved/df-compatible-release-asset-v1.bin`

Its verified SHA-256 is:

`c94d91f70911001c946e0fabb4aa9adc37045f45a03b56008cb0c8244cb63616`

It is the byte-for-byte upstream `Rikorose/DeepFilterNet` tag `v0.5.6` standard `DeepFilterNet3_onnx.tar.gz`, not `DeepFilterNet3_ll_onnx.tar.gz`, not a Large model, and not a converted or quantized derivative. The recorded archive size is `7,983,136` bytes. The archive’s known listing contains `enc.onnx`, `erb_dec.onnx`, `df_dec.onnx`, and `config.ini`; extraction must be defended against traversal, links, devices, executables, truncation, and archive corruption.

The authoritative approval chain is:

- `governance/model-assets/df-compatible-release-asset-v1/candidate-provenance.json`
- `governance/model-assets/df-compatible-release-asset-v1/legal-review.json`
- `governance/model-assets/df-compatible-release-asset-v1/approval-manifest.json`
- `governance/model-assets/df-compatible-release-asset-v1/approver-public-key.pem`
- fixed policy `governance/model-assets/trust-policy.json`
- current evidence `docs/evidence/m0-asset-gate.json`

The sole authoritative verification command is the production command in `docs/release-gates.md`:

```sh
python3 scripts/verify-m0-asset.py --candidate governance/model-assets/df-compatible-release-asset-v1/candidate-provenance.json --legal-review governance/model-assets/df-compatible-release-asset-v1/legal-review.json --approval governance/model-assets/df-compatible-release-asset-v1/approval-manifest.json --asset vendor/approved/df-compatible-release-asset-v1.bin --approver-key governance/model-assets/df-compatible-release-asset-v1/approver-public-key.pem --status-out docs/evidence/m0-asset-gate.json
```

It must exit zero and replace the evidence with `M0_APPROVED`. The verifier already enforces stable regular non-symlink snapshots; filename; asset SHA-256; exact records; canonical record digests; fixed trust policy; DER-SPKI key ID; and Ed25519 signature. The model implementation must call or faithfully port this exact verification contract, including the fixed policy path. It must not accept caller-selected keys, trust policies, manifests, URLs, alternate archives, or a cached status alone.

`ApprovedAssetManifest` is a typed, immutable runtime projection of the verified `ApprovedVerification` result, not a second manifest format or source of truth. It contains only values already bound by M0: `asset_id`, verified archive SHA-256, candidate-record SHA-256, legal-review-record SHA-256, authorized `key_id`, and the verified archive path/snapshot identity needed for safe loading. Construction is private to the M0 verification adapter and requires a fresh successful verification; no public literal, deserialization-only constructor, or `Default` exists. Revalidation must precede each backend construction/load, so replacing the path after an earlier check is rejected. The M0 JSON files and signed approval remain authoritative.

## Allowed Scope

The implementation worker may modify only the following Task 4 paths:

- `Cargo.toml`
- `crates/model/**`
- `crates/tools/src/bin/generate-golden.rs`
- `crates/tools/src/bin/benchmark.rs`
- `crates/tools/src/bin/validate-asset.rs`
- `fixtures/golden/**`
- `fixtures/corpus/README.md`
- `benchmarks/cpu-baseline.json`
- `benchmarks/cpu-baseline.md`

`benchmarks/cpu-baseline.json` is mandatory even though the Task 4 file list names only the Markdown file: Task 4 Step 4 explicitly requires the benchmark command to produce the JSON evidence. The JSON is the machine-readable gate input; the Markdown is a rendered human-readable summary of the same run and must cite the JSON file and its digest. Do not make the Markdown a separately maintained set of metrics.

Do not modify the plan, design, M0 governance records, trust policy, signed approval, archive, release-gate script, Task 3 contracts, fixtures outside `fixtures/golden`, or any other path. Do not access the network, add an unpinned dependency, regenerate a lockfile with network access, or update the lockfile until the proposed dependencies have passed the existing offline policy.

## Public Boundary

`crates/model` consumes only `AudioFrame` from `realtime-noise-contracts`. It must not introduce engine, transport, endpoint, IPC, device, UI, or raw-audio fallback behavior.

The public API must remain limited to these concepts; exact Rust field types and dependency-specific types are decided only after offline tract/libDF API research:

```rust
pub trait InferenceBackend: Send {
    fn descriptor(&self) -> BackendDescriptor;
    fn process(&mut self, input: &AudioFrame) -> Result<ProcessedFrame, InferenceError>;
    fn algorithmic_latency_samples(&self) -> u32;
}
```

`BackendDescriptor` identifies the backend as the CPU `tract` baseline and records immutable, machine-readable identity sufficient to bind a golden and benchmark to the verified model: backend kind/version, model asset ID, verified archive SHA-256, CPU profile, and model/runtime version identifiers. It must not claim a promoted accelerator or an unmeasured latency.

`ProcessedFrame` is the one processed `AudioFrame` plus only backend-produced metadata needed by later latency reporting: algorithmic latency in samples and a deterministic provenance/version reference. It must reject non-finite output; it does not carry timestamps, queue state, raw fallback data, or endpoint concerns.

`ApprovedAssetManifest` is the private-construction projection described above. `TractBackend::new` accepts it by value or validated reference and must fail closed before model/archive parsing if it is absent, invalid, stale, or does not bind the exact archive.

`InferenceError` must distinguish at least: `AssetNotApproved`, archive/manifest validation failure, unsupported CPU profile, model/archive corruption or truncation, input contract mismatch, non-finite output, inference execution failure, and deadline/measurement failure. Do not expose a generic success after suppressing one of these causes.

Latency reporting is a value output from the benchmark harness, not a model-side estimate: per-hop elapsed samples, p50/p95/p99, maximum, deadline misses, warm-up policy, and allocation measurement availability/count. The report labels values as measured, configured, derived, or not observable per the binding design. Task 4 reports worker inference only; it must not label an unmeasured end-to-end number as latency.

Golden comparison is deterministic and ordered by corpus case, model SHA-256, backend descriptor, output frame index, and sample index. It compares finite `f32` samples under frozen absolute/relative numerical tolerances and freezes the separate quality metric name, version, threshold, and input normalization. Every tolerance must be written with the generated fixture and provenance; no runtime adaptive threshold, architecture-dependent waiver, or broad `epsilon` is permitted.

## Required TDD Sequence

### Pre-RED guard

Before source or test implementation, create and register `crates/model/Cargo.toml` in root `workspace.members`, then run only offline Cargo commands. The manifest must preserve workspace lints and use `default-features = false` for every third-party dependency. If the pinned toolchain or dependency cache is unavailable, stop with `BLOCKED_OFFLINE_DEPENDENCY`; do not fetch.

First re-run the production M0 command above. If it does not produce `M0_APPROVED`, stop with `BLOCKED_NO_APPROVED_ASSET`. This is not a Task 4 test and must not alter the governance evidence in this preflight task.

### RED tests that are valid offline

Write integration tests before production model code. They must compile/run offline and use real parser/validation code plus deliberately invalid local fixture copies, not mock weights or a mocked inference result:

- `tract_backend_refuses_unapproved_asset`: an absent, unsigned, mismatched, stale, untrusted-key, or path-substituted approval input returns `InferenceError::AssetNotApproved`; backend construction never reaches model parsing.
- Asset binding tests: archive hash mismatch, signature mismatch, candidate/legal record digest mismatch, malformed/duplicate/non-finite JSON, wrong filename, symlink/non-regular path, and replacement between verification and open all fail closed.
- Archive safety tests: truncated/corrupt archive, traversal member, absolute member, symlink/hardlink/device/executable member, missing required ONNX/config entries, and altered member checksum fail before inference.
- Contract tests: only `AudioFrame` length 480 at mono 48 kHz is accepted; malformed shape/rate/channel metadata fails, never coerces silently.
- Output safety tests: a backend-produced NaN, infinity, or non-finite intermediate/output is rejected. This test may use a minimal deterministic test backend that directly returns non-finite output; it must not pose as tract or use mock weights.
- CPU capability tests: a profile lacking AVX2 is rejected with `UnsupportedCpuProfile` before backend activation.
- Golden reader/comparator tests: wrong provenance digest, wrong model digest, mismatched descriptor, missing case, truncation, non-finite fixture samples, reordered frames, numeric drift outside frozen tolerances, and quality-metric threshold drift fail deterministically.
- Evidence parser tests: stale timestamps/run identity and missing allocation measurement cannot be reported as a qualified baseline.

Run focused RED commands and record their nonzero status and expected missing-symbol/fixture failure. RED is valid only when it fails because the backend, verifier adapter, or golden fixture does not exist, not because a dependency is downloaded, a toolchain is missing, a test has bad imports, or an unrelated workspace failure occurs. No mocked model weights, synthetic "tract" outputs, or self-derived expected golden values are valid RED or GREEN evidence.

### Golden and benchmark are separate physical gates

The frozen-golden integration test cannot turn green until a designated isolated generation host has the exact approved archive, a validated local corpus, the resolved offline tract/libDF toolchain, and a successful M0 recheck. If any prerequisite is missing, retain the parser/security RED tests, write durable `BLOCKED_PENDING_GOLDEN` evidence, and do not create a placeholder golden.

The 300-second benchmark cannot qualify on CI emulation, a VM without verified topology, an unknown CPU, an Apple Silicon host, or a CPU faster than the stated reference class. It requires a physical x86_64 AVX2 host with four physical cores, sustained 2.0 GHz on AC, at least 8 GB RAM, and a class no faster than Intel Core i5-10210U. Apple Silicon needs separate CPU characterization and cannot approve this `avx2-minimum` baseline. An unsupported or unverifiable CPU profile is `BLOCKED_UNSUPPORTED_CPU_PROFILE`, not a skipped success.

### GREEN order

1. Implement only sufficient M0-bound asset validation and `ApprovedAssetManifest` construction to pass the asset/security tests.
2. Implement only sufficient safe archive parsing and CPU profile preflight to pass archive and capability tests.
3. Resolve tract/libDF APIs from locally available official documentation/source and offline crate metadata. Record exact package versions, features, model input/output contract, state/reset behavior, thread safety, allocator behavior, and any unsafe/transitive native requirements before adding code. Do not guess tract APIs from this brief.
4. Implement the minimal `TractBackend` and finite-output boundary required to process one `AudioFrame`.
5. On the approved isolated host only, generate the corpus golden through `generate-golden`; freeze provenance before adding the golden equivalence test’s final fixture assertion.
6. Run model unit/integration tests offline. Then run the physical benchmark once all its measurements are available.

## Golden and Corpus Evidence

`fixtures/corpus/README.md` must replace the current Task 1 placeholder without adding corpus audio to the repository. For every local corpus item, it records a stable case ID, immutable local origin, checksum, license, redistribution terms, consent/authorization where applicable, transcription applicability, SNR/noise class, and declared sample rate/channel format. It must cover the binding design’s adult male/female voices, authorized child speech where used, stationary noise, keyboard, traffic, fan, music, competing speech, reverberation, SNR levels, and silence. Model approval and corpus approval remain separate gates.

`fixtures/golden/README.md` must define the fixture format and enumerate each case. The frozen golden data and README must include:

- generator binary/source revision and command identity;
- generation UTC timestamp, host OS/CPU identity, and isolated/offline status;
- exact archive SHA-256, M0 candidate/legal/approval digest bindings, authorized key ID, and backend descriptor;
- per-input corpus checksum and ordered frame sequence;
- output checksum, frame count, sample rate/channels/hop size, algorithmic latency samples;
- numerical comparator algorithm and exact absolute/relative tolerances;
- quality metric implementation/version, input window/normalization, threshold, and observed reference value;
- fixture schema/version and a digest of the entire provenance record.

The generator must refuse to overwrite a frozen fixture unless an explicit fresh-generation mode records a new provenance identity; normal test runs are read-only. A stale corpus checksum, asset digest, backend descriptor, generator version, schema version, or missing provenance is `BLOCKED_PENDING_GOLDEN`, not a tolerance adjustment. Golden comparison must never use the just-produced output as its expected value.

## Benchmark Qualification

Run only after a valid golden exists:

```sh
cargo run -p realtime-noise-tools --bin benchmark -- --backend tract --profile avx2-minimum --duration 300
```

The command must run exactly 300 seconds of measured workload after separately reported warm-up. It must use the frozen corpus/golden workload and the exact approved asset, not random or mock weights. The benchmark must verify its own backend/profile selection and reject misleading output: exit status, structured status, selected descriptor, asset/golden digests, actual elapsed duration, CPU evidence, and allocation instrumentation availability must agree. A friendly line claiming success cannot override a nonzero exit, missing JSON, or failing structured gate.

`benchmarks/cpu-baseline.json` is the canonical machine-readable report and must contain:

- schema/version, run ID, UTC timestamps, command/arguments, git revision and clean-worktree status;
- host OS/kernel, CPU model/topology/physical-core count, AVX2 evidence, sustained-frequency/AC evidence, RAM, reference-class attestation, and process affinity/governor information when observable;
- model approval binding, backend descriptor, corpus/golden provenance digests, warm-up policy, exact measured duration;
- count, p50/p95/p99/max per-hop latency, `deadline_ms: 10`, deadline-miss count, `p99_limit_ms: 7`, and pass/fail decision;
- allocator measurement mechanism, availability, initialization/warm-up allocations separated from per-hop allocations, per-hop allocation count/bytes or an explicit unavailable state;
- explicit status: `M1_APPROVED`, `M1_FAILED_BENCHMARK`, `BLOCKED_NO_APPROVED_ASSET`, `BLOCKED_PENDING_GOLDEN`, `BLOCKED_UNSUPPORTED_CPU_PROFILE`, or `BLOCKED_ALLOCATION_MEASUREMENT`.

`benchmarks/cpu-baseline.md` summarizes that one JSON report, cites the JSON digest, and reproduces the decision/reasons without new metrics. It must state that the report measures inference-worker latency only and cannot claim product end-to-end latency.

Qualification requires all conditions below:

- backend exactly `tract` against the M0-bound asset;
- profile exactly `avx2-minimum` on the physical qualifying host;
- duration exactly 300 seconds measured after warm-up;
- p99 `<= 7 ms`;
- every measured hop `<= 10 ms` and deadline misses equal zero;
- per-hop allocation instrumentation is present and reports its result; target is zero allocations/hop, but any nonzero result requires an explicit measured, bounded approval decision not present in this task brief;
- provenance, corpus, golden, host, and clean-worktree checks all pass.

Failure of any requirement writes a truthful failure/block result and blocks M1. It never changes thresholds, downgrades duration, substitutes a faster host, or promotes an accelerator/fallback.

## Adversarial and Operational Checks

Before declaring Task 4 complete, cover and document these classes:

- asset hash/signature/manifest tampering, untrusted key, stale M0 evidence, inconsistent record digests, and alternate asset IDs;
- archive/model path substitution before or after verification, symlinks and non-regular files;
- corrupt/truncated archive/model and unsafe archive members;
- golden provenance drift, corpus checksum drift, descriptor/model-digest drift, precision/quality drift, reordered frames, and stale fixtures;
- NaN, infinity, non-finite intermediate/output, malformed shape/rate/channel input, and unexpected model I/O;
- absent AVX2, false/unsupported CPU profile, VM/container topology ambiguity, and Apple Silicon baseline misuse;
- absent, disabled, or ambiguous allocation instrumentation;
- incomplete/short benchmark, stale report, reported duration mismatch, misleading command text, nonzero process status, and missing JSON/Markdown pair;
- offline/no-network behavior, unavailable pinned toolchain/cache, and dependency resolution attempting network access;
- dirty worktree before generation/benchmark, including generated output outside the allowed paths;
- temporary extraction/generation/benchmark resources, files, processes, allocations/profilers, and containers left behind.

## Cleanup and Blocking Receipt

The implementation worker must remove temporary archive extraction trees, generated scratch models, corpora copies, profile dumps not designated as Task 4 evidence, temporary keys, test-only model artifacts, benchmark processes, and disposable containers. Retain only allowed frozen goldens, corpus provenance README, benchmark JSON/Markdown, code/tests, and Cargo changes. Verify no temporary resource remains and `git status --short --untracked-files=all` names only intended Task 4 changes before its own commit.

Use these explicit outcomes:

- `BLOCKED_NO_APPROVED_ASSET`: fresh M0 verification fails, the archive/record/key binding is absent or invalid, or construction cannot obtain a valid manifest.
- `BLOCKED_PENDING_GOLDEN`: approved asset exists but validated corpus, isolated golden generation host, frozen provenance, or deterministic golden fixture is unavailable or stale.
- `BLOCKED_OFFLINE_DEPENDENCY`: pinned Rust/tooling or an approved dependency is absent from the offline environment.
- `BLOCKED_UNSUPPORTED_CPU_PROFILE`: the benchmark host cannot prove the required physical AVX2 qualification.
- `BLOCKED_ALLOCATION_MEASUREMENT`: allocation instrumentation is unavailable, ambiguous, or cannot separate warm-up from per-hop measurements.
- `M1_FAILED_BENCHMARK`: a valid physical run violates duration, p99, deadline, allocation, or provenance requirements. This blocks M1 and downstream engine/AUTO/adapter work.

Only an evidence-complete, qualified run may record `M1_APPROVED`.

## Implementation Research Questions

Resolve these against locally available official source/docs and offline metadata before coding; do not infer answers from upstream memory or network access:

- Which `libDF` crate/version is available offline, its license/features/native build requirements, and how it consumes this exact DFN3 archive.
- Which `tract` crate/version/features are cached, whether its ONNX loader and execution plan work with all three archive models, and the exact safe API for typed `f32` tensor I/O.
- The model’s recurrent/state, STFT/iSTFT, normalization, lookahead, hop framing, algorithmic latency, reset, and warm-up contract; a one-hop API is insufficient if hidden state is required.
- How to prevent all non-finite input/output states and report typed errors without unsafe code under the workspace lints.
- A platform-supported allocation-measurement technique for the qualifying Linux/x86_64 host that distinguishes initialization from each worker hop without changing inference behavior.
- The reproducible quality metric and version suitable for the frozen corpus, including licensing and offline availability.
- The exact CPU inventory/frequency/governor evidence source and how it detects a VM, hyperthread count, or a host faster than the reference class.

If any answer is unavailable, create no speculative production API or artifact. Record the matching block status and stop at the relevant gate.

## Acceptance Checklist

- [ ] Fresh M0 command passed against the exact tracked archive, or Task 4 stopped as `BLOCKED_NO_APPROVED_ASSET`.
- [ ] Model crate registered before package commands; all dependencies are explicit `default-features = false` and offline-resolvable.
- [ ] RED failures were observed for asset gating and golden equivalence before their production implementation, without mock weights.
- [ ] `InferenceBackend`, descriptor, processed output, manifest projection, typed errors, and latency reporting remain within Task 4’s boundary.
- [ ] Asset, archive, CPU, finite-output, golden, and evidence adversarial tests pass offline where physically possible.
- [ ] Corpus and golden provenance/tolerances are frozen only on the isolated approved-asset host, or `BLOCKED_PENDING_GOLDEN` is recorded.
- [ ] Both `benchmarks/cpu-baseline.json` and `benchmarks/cpu-baseline.md` exist for a qualified run and agree by JSON digest.
- [ ] A physical `avx2-minimum` run lasted 300 seconds, measured allocations per hop, met p99/deadline gates, and recorded `M1_APPROVED`; otherwise M1 is blocked.
- [ ] Temporary resources are removed and only Task 4 allowed paths changed.
