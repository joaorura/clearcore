## Description
<!--
  Provide a concise summary of the changes in this PR.
  Explain the problem solved, design choices, architectural tradeoffs, and references to relevant issues or specifications.
-->

Closes #<!-- issue number -->

## Type of Change
<!-- Please check all that apply: -->
- [ ] `feat`: New feature or user-facing capability
- [ ] `fix`: Bug fix or patch
- [ ] `perf`: Performance or latency optimization
- [ ] `docs`: Documentation update or addition
- [ ] `test`: New or modified tests/fixtures/qualification harnesses
- [ ] `refactor`: Internal code reorganization with no behavior change
- [ ] `chore`: Build configuration, dependencies, or maintenance
- [ ] `ci`: CI/CD workflows, gates, or packaging scripts

## Platform Scope
- [ ] Cross-platform / Core Rust workspace (`engine`, `service`, `supervisor`, `model`, `ipc`, `diagnostics`)
- [ ] Linux (PipeWire C bridge, WirePlumber rules, desktop integration)
- [ ] Windows (WaveRT SysVAD kernel driver, PortCls, Inno Setup/NSIS)
- [ ] macOS (CoreAudio HAL AudioServerPlugIn, package notarization)
- [ ] Desktop UI (Electron/Tauri frontend, tray manager, settings)

---

## Testing & Verification Performed
<!--
  Describe the tests you ran to verify your changes. Include hardware specs and platform configurations.
-->
- **Local Commands Executed**:
  - [ ] `cargo fmt --all -- --check`
  - [ ] `cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings`
  - [ ] `cargo test --workspace --features tract`
  - [ ] `./scripts/check-offline.sh` (Offline qualification gate)
  - [ ] `bash tests/check-offline.sh` (Offline gate regression harness)
- **Host Platform(s) Tested**:
  - [ ] Linux: Distribution & Kernel: `________________________`
  - [ ] Windows: Build version: `________________________`
  - [ ] macOS: OS version & architecture: `________________________`
- **Driver Verification**:
  - [ ] `./scripts/check-virtual-mic.sh --status` verified healthy virtual audio endpoint.
  - [ ] Soak test verified zero audio dropouts over extended run.

---

## Architectural & Security Checklist
<!--
  All PRs must satisfy ClearCore's architectural invariants:
-->
- [ ] **Memory Safety**: Strict `#![forbid(unsafe_code)]` is preserved across all Rust workspace crates (or any additions in `crates/filter-capi` have explicit `// SAFETY:` invariants, are bounded, and pass Clippy).
- [ ] **Fail-Closed Silence Policy**: Verified that under any failure, crash, buffer underrun, or disconnect, the pipeline outputs **digital silence (zeros)** and never leaks raw microphone audio.
- [ ] **Realtime Audio Callback Contract**:
  - [ ] Zero dynamic heap allocations in audio processing loops (`malloc`, `Vec`, `Box`, `String`).
  - [ ] Zero blocking I/O, syscalls, or lock contention in the audio thread.
- [ ] **Deterministic Offline Build**: No network egress during build or test execution; dependencies conform to `.cargo/config.toml` locked cache.
- [ ] **Latency Budget**: Inference remains <= 7.0 ms p99 per 10 ms hop; end-to-end pipeline remains <= 70.0 ms.
- [ ] **Diagnostics & Privacy**: No raw PCM audio, speech transcripts, or high-dimensional embeddings are serialized in diagnostic archives.
- [ ] **Conventional Commits**: PR and commit titles follow the Conventional Commits specification.
- [ ] **Documentation**: Updated `README.md`, `AGENTS.md`, or `docs/` where applicable.
