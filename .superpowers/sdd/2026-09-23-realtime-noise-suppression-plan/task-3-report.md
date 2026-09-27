# Task 3 implementation report

## Result

Implemented the dependency-free `realtime-noise-contracts` crate for the M1 audio, wire, and
transport contracts. The implementation is limited to Task 3: it contains no production queue,
engine recovery, watermark policy, metrics, silence policy, adapters, resampling, IPC, or Task 4+
behavior.

Requirements were taken from `task-3-brief.md` and binding design lines 82-180 and 198-206.

## Baseline characterization

- Required baseline HEAD: `c342a42b200f1d1864a03ff634e4338cb82c5562`.
- Observed baseline HEAD: exact match.
- `git status --short --untracked-files=all`: empty.
- Runner: Rust 1.90.0 (`rustc 1.90.0`, Cargo 1.90.0, rustfmt 1.8.0-stable, Clippy 0.1.90).
- `cargo test --workspace --locked --offline`: exit 0; all 9 Task 2 policy tests passed.
- `cargo test -p workspace-policy --locked --offline`: exit 0; all 9 policy tests passed.
- `bash scripts/check-offline.sh`: exit 0.

The local `rust:1.90.0-bookworm` image initially lacked rustfmt and Clippy. Its first
`--network none` probe failed before project execution while rustup attempted a channel lookup.
A disposable local image was provisioned separately with network access only for
`rustup component add rustfmt clippy`; every project command then used `--network none` and
`RUSTUP_TOOLCHAIN=1.90.0`. Raw bootstrap evidence is in `task-3-evidence/runner-bootstrap.txt`.

## TDD evidence

### Audio and wire RED

The crate manifest, workspace registration, empty `src/lib.rs`, and offline-generated lockfile
existed before RED. Comprehensive integration tests and independent fixtures were then added.

Command:

`cargo test -p realtime-noise-contracts --test wire_v1 --locked --offline`

Observed exit status: 101.

Expected failure: unresolved imports for `AudioFrame`, constants, `Discontinuity`,
`FrameEnvelope`, `WireDecodeError`, and `WireFrameEnvelopeV1`. No production Task 3 API existed.

The first post-implementation compile exposed a test-harness type error: comparing
`Result<FrameEnvelope, _>` required `FrameEnvelope: PartialEq`. The harness was corrected to
compare `.err()` values and use `Result`-returning tests; production semantics were unchanged.
The next focused run passed all 12 wire/audio tests.

### Transport RED

Command:

`cargo test -p realtime-noise-contracts --test transport_properties --locked --offline`

Observed exit status: 101.

Expected failure: unresolved imports for `DEFAULT_CAPACITY_HOPS`, `RealtimeTransport`, and
`TransportFull`. No transport production API existed.

After adding only the interface, constant, and typed error, the focused run passed all 6 transport
tests. The deterministic `Mutex<VecDeque<FrameEnvelope>>` implementation exists only in the test.

## Implemented contracts

- Mono 48 kHz, 480-sample `AudioFrame` hops.
- Public, directly constructible, `Copy` `FrameEnvelope`.
- Dependency-free checked `Discontinuity` newtype with NONE and bits 1, 2, 4, and 8.
- `WireFrameEnvelopeV1` layout witness with `repr(C, align(8))` and compile-time size, alignment,
  and all normative offset assertions.
- Explicit little-endian `encode`/`decode`; no unsafe code, transmute, pointer casts, or Rust-memory
  serialization.
- Fail-closed errors for total length, version, payload length, unknown flags, and reserved bytes,
  validated before sample decoding.
- Bit-exact IEEE-754 preservation for signed zero, infinities, and multiple NaN payloads.
- Exact object-safe `RealtimeTransport: Send + Sync`, `TransportFull`, and
  `DEFAULT_CAPACITY_HOPS = 24`; `close_generation` remains interface-only.

Every source/test file remains below 200 pure LOC: `lib.rs` 6, `audio.rs` 45, `wire.rs` 134,
`transport.rs` 17, `wire_v1.rs` 138, and `transport_properties.rs` 93.

## Fixture provenance

Fixtures were produced before production encoding code by a temporary standard-library-only Rust
generator outside the repository. The valid record has version 1, payload 1920, flags 5, reserved
zero, sequence 42, timestamp 1,000,000,000, generation 7, samples +0.0/-0.0/+1.0/-1.0, then zeros.
The invalid fixture changes only offset 12 to 1.

Both files are exactly 1960 bytes. `cmp -l` reports one difference at one-based byte 13.
The generator source, binary, and named container were removed. Raw evidence is in
`task-3-evidence/fixture-inspection.txt`.

## Verification

All project commands below ran in the isolated Rust 1.90 runner with networking disabled:

- `cargo fmt --all -- --check`: exit 0.
- Focused wire tests: 12 passed, 0 failed.
- Focused transport tests: 6 passed, 0 failed.
- Package tests: 18 integration tests passed, 0 failed.
- Workspace-policy manifest acceptance: 1 passed, 0 failed.
- Strict workspace Clippy with `-D warnings`: exit 0, no warnings.
- Workspace build: exit 0.
- Workspace tests: 27 passed, 0 failed.
- `scripts/check-offline.sh`: exit 0.
- Offline lockfile regeneration: byte-identical SHA-256 before/after
  `db2397f0083d9b97f9820037c532a33e1cff31943e2abfeee7ef297d1dcc47e6`;
  `git diff --exit-code -- Cargo.lock` returned 0.

The final combined matrix used `set -e` and returned 0. Full command evidence is in
`task-3-evidence/verification-matrix.txt`.

Rust LSP diagnostics were requested for every changed Rust file, but the configured Rust LSP is
not installed and installation was previously declined. Compiler, strict Clippy, formatting,
build, tests, and Rustdoc provide the available static verification evidence.

## Manual QA

- `wc -c`, `od`, and `cmp -l` evidence: `task-3-evidence/fixture-inspection.txt`.
- Generated Rustdoc public API inspection: `task-3-evidence/rustdoc-public-api.txt`.
- Temporary external path consumer: `task-3-evidence/external-consumer.txt`.
- The consumer decoded the valid fixture, re-encoded exact bytes, and decoded an independent next
  record as sequence 43, with output:
  `decoded sequence=42 generation=7 flags=5 bytes_exact=true next_sequence=43`.

## Adversarial probes

- `malformed_input`: applicable, passed all required malformed header/length/flag/reserved cases.
- `stale_state`: applicable, external consumer observed the second independent sequence as 43.
- `dirty_worktree`: applicable, baseline tracked status was empty; final receipt follows cleanup.
- `flaky_tests`: applicable, complete package ran three consecutive times with 18/18 passing.
- `misleading_success_output`: applicable, explicit command statuses matched success output and the
  combined gate reported status 0 only after all commands.
- `prompt_injection`: not applicable; no prompt/instruction input exists.
- `cancel_resume`: not applicable; no resumable operation exists.
- `hung_or_long_commands`: not applicable; no time-based or external-service waits exist.
- `repeated_interruptions`: not applicable; no persisted workflow exists.

Raw observations are in `task-3-evidence/adversarial-probes.txt`.

## Architecture self-review

- Single responsibility: each production module owns one concept (audio values, wire codec, or
  transport interface).
- Boundary purity: untrusted wire bytes are parsed once into checked domain values.
- Variants: `WireDecodeError` display matching is exhaustive.
- Escape hatches: no unsafe, unwrap, expect, transmute, pointer cast, or external dependency.
- Defensive layers: only wire-boundary checks required by the binding spec are present.
- Helpers: no production one-off abstractions were introduced.
- Parameter count: no modified function exceeds three parameters.
- Logging: none added, matching existing crate behavior.

## Commits

- `77a364058750a1a7aa40daaae0c553e7369c3e15` `feat: add realtime audio and wire contracts`
- This report's containing commit (`HEAD` after Task 3):
  `feat: add realtime transport contract`. Its exact SHA is reported in the completion receipt;
  embedding a commit's own SHA in its contents is self-referential.

## Cleanup receipt

- Fixture generator: source deleted from `/tmp/opencode`; binary and named container removed.
- RED setup: no temporary production stubs; RED tests remain as the regression suite; compiler
  scratch is confined to ignored `target/`.
- External consumer: source directory and named container removed after the offline run.
- Verification containers: all used `--rm` and left no containers.
- Scratch files: no Task 3 scratch source remains outside the durable evidence directory.
- Local runner image: removed after the second commit and final repository checks.

## Fix round 1

### Important finding

Independent review found that the public `Discontinuity` API did not match the intended wire-facing
contract: its storage and `bits`/`from_bits` signatures used `u32`, and consumers had no typed
`contains(Self)` operation. The V1 record still stores flags in a four-byte little-endian field, so
the correction is an API-domain `u8` value with an explicit `u8`/`u32` codec boundary, not a wire
layout change.

### RED and GREEN evidence

The durable original RED evidence remains above: before Task 3 production APIs existed,
`cargo test -p realtime-noise-contracts --test wire_v1 --locked --offline` exited 101 because the
required imports were unresolved. The original GREEN matrix in
`task-3-evidence/verification-matrix.txt` predates this round and is not claimed as a green result
for this correction.

For this round, the regression test now requires `u8` bits, rejects `16_u8`, and verifies
`contains(Self)` for present and absent flags. A fresh GREEN and full offline matrix were attempted
in the available disposable Rust 1.90-tagged runner with networking disabled. The runner lacked
both `rustc` and `cargo`, so the command exited 127 before any Cargo command ran. The exact command
and output are preserved in `task-3-evidence/fix-round-1.txt`; no successful fresh Cargo result is
invented here.

### API and codec correction

- `Discontinuity` now wraps `u8`; `KNOWN_BITS`, `bits`, and `from_bits` use the same domain type.
- `Discontinuity::contains(self, flag: Self)` exposes the typed consumer query.
- Encoding widens the typed value with `u32::from(...)` before writing the unchanged four-byte V1
  little-endian field.
- Decoding rejects non-normative `u32` wire bits, then narrows with `u8::try_from(...)` before
  constructing `Discontinuity`.

### Verification results

- Scoped working-tree inspection found only the three expected Rust paths modified.
- The code/test diff contains the `u8` domain contract, typed containment, explicit wire widening
  and narrowing, and the focused regression coverage.
- Fresh Rust verification is blocked by the local runner failure described above; this is a runner
  availability result, not a passing test claim.

### Manual consumer impact

Consumers now receive a `u8` from `bits`, pass a `u8` to `from_bits`, and query combined flags with
`contains(Self)`. The on-wire V1 flags remain four bytes and retain their little-endian ABI, so
records do not change layout or encoding width.

### Cleanup and quota fallback

- The disposable verification containers used `--rm`; no container is retained by this attempt.
- The original OpenAI worker exhausted quota after implementation; the fallback worker supplied the
  scoped code/test changes but did not create a commit. This round packages those changes without
  broadening the fix.
- The temporary `hippocamp-rust:1.90-task3-fix1` runner image is removed during the final cleanup
  receipt.
