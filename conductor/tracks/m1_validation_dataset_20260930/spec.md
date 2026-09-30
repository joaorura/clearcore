# Specification: M1 Filter Validation Dataset & Task 4 Golden Harness Resolution

## 1. Overview
This track delivers the full resolution of Task 4 in Wave 1 of the Hippocamp roadmap. It establishes the authoritative M1 Qualification Corpus (`fixtures/corpus/`), generates the frozen reference filter dataset (`fixtures/golden/frozen-reference.json`), and captures machine-verifiable benchmark evidence on the reference CPU backend (`tract` DeepFilterNet3 v0.5.6) to unlock the `M1_APPROVED` gate.

## 2. Functional Requirements
1. **Schema V1/V2 Manifest Finalization:**
   - Commit and verify the extracted `golden_manifest` module in `crates/tools/src/golden_manifest.rs`.
   - Ensure backward compatibility with `ManifestV1` (only `cases` hashed) and full governance coverage with `ManifestV2` (hash covering all metadata + `source_lock_sha256`).
   - Fail closed with `BLOCKED_SCHEMA_MISMATCH` on unrecognized or improperly bound schema fields.
2. **Speech & Noise Source Acquisition & Lock:**
   - Integrate verified adult speech sources (EdAcc v1.0, CC-BY 4.0, Edinburgh Accents) covering male and female voices with adult age verification.
   - Bind existing candidate noise assets (Wikimedia Commons keyboard, fan, traffic, piano, deterministic digital silence, synthetic stationary noise, and Zenodo HOMULA-RIR).
   - Generate `fixtures/corpus/source-lock.json` capturing immutable URLs, exact byte sizes, and SHA-256 digests.
3. **Deterministic Audio Normalization & Framing:**
   - Resample and normalize audio sources to 48 kHz mono `f32` with hop size = 480 samples.
   - Format test cases into JSON frame structures without storing raw audio binaries in Git.
4. **Golden Reference Generation:**
   - Execute `generate-golden --fresh` in Docker-isolated offline container (`hippocamp-task4-rust:local`).
   - Validate numerical tolerance boundaries against the frozen model output.
5. **Algorithmic Latency Alignment:**
   - Formally document the 30 ms (1,440 samples @ 48 kHz) algorithmic latency contract to align the design specification with the physical DeepFilterNet3 v0.5.6 ONNX model.
6. **Benchmark & Hardware Evidence:**
   - Run CPU benchmark qualification harness with structured 300-second sustained frequency observations and record the development host hardware waiver.

## 3. Non-Functional Requirements & Security
- **Offline Integrity:** All generation and validation commands must execute with `--network none` and `--locked --offline`.
- **Legal Compliance:** Strict provenance verification; zero unlicensed or ambiguously licensed training/test audio.
- **Fail-Closed Execution:** Any digest mismatch or missing source lock must abort generation with exit code 2.

## 4. Acceptance Criteria
- [ ] `cargo test -p realtime-noise-tools --locked --offline` passes 100% of test suites.
- [ ] `crates/tools/src/bin/generate-golden.rs` successfully compiles and runs with `--fresh`, creating `fixtures/golden/frozen-reference.json`.
- [ ] `fixtures/corpus/corpus-manifest.json` exists, is verified with schema v2, and matches `fixtures/corpus/source-lock.json`.
- [ ] `task-4-corpus-builder-report.md` updated with confirmed passing audit verdicts.
- [ ] No regression on existing workspace contracts (`crates/contracts`, `crates/model`, `crates/workspace-policy`).

## 5. Out of Scope
- Task 5 (Engine ring buffer queues and silence generation).
- Platform audio driver implementations (Windows WaveRT, macOS CoreAudio, Linux PipeWire).
- Hardware accelerator backends (TensorRT, MIGraphX, CoreML).
