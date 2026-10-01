# Contributing to ClearCore

Thank you for your interest in contributing to **ClearCore** (formerly project Hippocamp)! ClearCore is an open-source, first-party, realtime noise-suppression virtual microphone engineered for mission-critical voice communications. It is powered by DeepFilterNet3 ONNX/tract, a Linux PipeWire C bridge, a Windows WaveRT PortCls driver, a macOS CoreAudio HAL AudioServerPlugIn, and an asynchronous Rust supervisor daemon.

We welcome contributions from developers, audio engineers, systems programmers, and documentation specialists. This document outlines our architectural contracts, coding standards, development setup, and pull request procedures.

---

## Table of Contents

- [Code of Conduct](#code-of-conduct)
- [Architecture & Core Engineering Principles](#architecture--core-engineering-principles)
  - [1. Strict Memory Safety & `#![forbid(unsafe_code)]`](#1-strict-memory-safety---forbidunsafe_code)
  - [2. Fail-Closed Digital Silence Policy](#2-fail-closed-digital-silence-policy)
  - [3. Hard Realtime Callback Guarantees (Zero Heap Allocations)](#3-hard-realtime-callback-guarantees-zero-heap-allocations)
  - [4. Deterministic Offline Qualification Gates](#4-deterministic-offline-qualification-gates)
- [Development Environment Setup](#development-environment-setup)
  - [Prerequisites](#prerequisites)
  - [Running the Development Environment](#running-the-development-environment)
  - [Building Production Packages](#building-production-packages)
  - [Running Tests & Quality Checks](#running-tests--quality-checks)
  - [Platform-Specific Driver Verification](#platform-specific-driver-verification)
- [Commit & Pull Request Guidelines](#commit--pull-request-guidelines)
  - [Conventional Commits](#conventional-commits)
  - [Pull Request Process & Review Checklist](#pull-request-process--review-checklist)

---

## Code of Conduct

All contributors, maintainers, and community members are expected to uphold the [Contributor Covenant Code of Conduct v2.1](CODE_OF_CONDUCT.md). Please report unacceptable behavior to [community@clearcore.ai](mailto:community@clearcore.ai).

---

## Architecture & Core Engineering Principles

ClearCore operates in privileged kernel and driver spaces, processing continuous live microphone audio with strict real-time deadlines. Every contribution must respect four non-negotiable architectural invariants:

### 1. Strict Memory Safety & `#![forbid(unsafe_code)]`

- All Rust workspace crates strictly enforce `#![forbid(unsafe_code)]` via workspace lints in `Cargo.toml`.
- The **only** workspace exception is `crates/filter-capi`, which provides the standard C ABI interface to OS audio drivers (e.g., PipeWire, SysVAD, and CoreAudio HAL). Any `unsafe` block in `crates/filter-capi` must:
  - Be accompanied by a detailed safety invariant justification (`// SAFETY: ...`).
  - Pass Clippy with `undocumented_unsafe_blocks = "deny"`.
  - Undergo AddressSanitizer (ASan) and UndefinedBehaviorSanitizer (UBSan) verification.
- No `unsafe` code will be accepted anywhere else in the workspace.

### 2. Fail-Closed Digital Silence Policy

- **Zero Raw Audio Leakage Guarantee**: Under no circumstance may unprocessed raw microphone audio leak into the output virtual microphone endpoint during an error state, DSP failure, thread crash, buffer underrun, model initialization delay, or supervisor restart.
- If processing is interrupted or unready, the audio pipeline MUST emit **digital silence (zeros)**. 
- Mute and Bypass states are governed by explicit, atomic state transitions verified through offline integration tests.

### 3. Hard Realtime Callback Guarantees (Zero Heap Allocations)

- The audio processing callback runs inside the high-priority OS audio thread (WASAPI/WaveRT, PipeWire process callback, or CoreAudio IO server).
- **Prohibited in the audio callback:**
  - Dynamic heap allocations (`malloc`, `free`, `Box::new`, `Vec::push`, `String`).
  - Unbounded locks or synchronization primitives that can lead to priority inversion (`std::sync::Mutex`, condition variables).
  - Synchronous blocking I/O, file access, IPC queries, or network operations.
  - Panics, exceptions, or fatal exits.
- All ring buffers, circular queues, and scratch buffers must be pre-allocated during pipeline initialization before starting the audio stream.

### 4. Deterministic Offline Qualification Gates

- ClearCore operates under an air-gapped, deterministic build and qualification model.
- CI runners and qualification scripts execute with `--locked`, `--offline`, and `--network none`.
- All production release assets must match verified SHA-256 provenance manifests (`governance/model-assets/`).
- Code changes must never assume or require internet connectivity during compilation, test execution, or packaging.

---

## Development Environment Setup

### Prerequisites

| Tool | Recommended Version | Purpose |
|---|---|---|
| **Rust Toolchain** | `1.90.0` (pinned) | Workspace compilation, Clippy, rustfmt (see `rust-toolchain.toml`) |
| **Node.js & npm** | Node `v20.x` LTS+ / npm `10.x`+ | Desktop application UI (`crates/app-tauri`) |
| **Meson & Ninja** | Meson `1.0+` / Ninja `1.11+` | Linux PipeWire helper C bridge (`platform/linux/helper`) |
| **Docker** | Latest standard engine | Hermetic offline verification gate (`scripts/check-offline.sh`) |
| **Platform SDKs** | OS-dependent | Windows: MSVC & Windows Driver Kit (WDK) 10/11<br>macOS: Xcode Command Line Tools & AudioServerPlugIn SDK<br>Linux: `libpipewire-0.3-dev`, `libspa-0.2-dev`, `wireplumber` |

### Running the Development Environment

ClearCore includes integrated scripts that launch both the backend supervisor daemon and the Vite/Electron/Tauri frontend with live Hot Module Replacement (HMR).

#### Linux & macOS:
```bash
./dev.sh
```
*Alternatively, run from the app directory:*
```bash
cd crates/app-tauri && npm run dev
```

#### Windows (CMD & PowerShell):
```cmd
dev.bat
```
*or in PowerShell:*
```powershell
.\dev.ps1
```

#### Frontend UI Only (Browser Mode):
To rapidly develop UI components without launching background native drivers:
```bash
cd crates/app-tauri && npm run dev:ui
```

### Building Production Packages

To build production binaries, installers, and drivers across platforms:

- **Linux / macOS:**
  ```bash
  ./package.sh
  ```
  - Outputs on Linux: `release/Clearcore-linux-x64/` (standalone executable + `install.sh`) and `.tar.gz`.
  - Outputs on macOS: `release/Clearcore-darwin-x64/` (`Clearcore.app` with embedded `RealtimeNoiseHAL.driver`).

- **Windows:**
  ```cmd
  package.bat
  ```
  - Outputs: `release\Clearcore-win32-x64\` plus Inno Setup (`Clearcore-Setup.iss`) and NSIS (`Clearcore-Setup.nsi`) single-click installers.

### Running Tests & Quality Checks

Before submitting any code, verify that all formatters, linters, tests, and offline gates pass locally:

1. **Format & Linting:**
   ```bash
   cargo fmt --all -- --check
   cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings
   ```

2. **Workspace Test Suite:**
   ```bash
   cargo test --workspace --features tract
   ```

3. **Deterministic Offline Gate:**
   ```bash
   ./scripts/check-offline.sh
   ```
   *Runs locked, offline metadata resolution, compilation, and test execution.*

4. **Offline Gate Regression Harness:**
   ```bash
   bash tests/check-offline.sh
   ```

### Platform-Specific Driver Verification

To test and inspect the status of the virtual microphone driver on your local host:

- **Linux:**
  ```bash
  ./scripts/check-virtual-mic.sh --status
  ```
- **macOS:**
  ```bash
  ./scripts/check-virtual-mic-macos.sh --status
  ```
- **Windows (PowerShell):**
  ```powershell
  powershell -ExecutionPolicy Bypass -File .\scripts\check-virtual-mic-windows.ps1 -Status
  ```

---

## Commit & Pull Request Guidelines

### Conventional Commits

We follow the [Conventional Commits v1.0.0](https://www.conventionalcommits.org/) specification. Commits should be structured as follows:

```text
<type>(<scope>): <short description>

[optional body explaining motivation and architectural tradeoffs]

[optional footer: Fixes #123]
```

#### Allowed Types:
- `feat`: A new user-facing feature or capability.
- `fix`: A bug fix or patch.
- `perf`: A code change that improves latency, CPU usage, or memory footprint.
- `docs`: Documentation updates or additions.
- `test`: Adding or updating test cases, fixtures, or benchmarks.
- `refactor`: Code change that neither fixes a bug nor adds a feature.
- `chore`: Build system, toolchain, or dependency updates.
- `ci`: Changes to CI/CD workflows or release gates.

#### Common Scopes:
- `engine`: Realtime DSP, circular buffers, hop chunking.
- `model`: ONNX inference, Tract engine, weight validation.
- `service`: Daemon supervisor, lifecycle management, autostart.
- `ipc`: Protocol serialization, socket/named-pipe communication.
- `pipewire`: Linux PipeWire C helper and WirePlumber integration.
- `wavert`: Windows kernel-mode audio driver / PortCls.
- `hal`: macOS CoreAudio AudioServerPlugIn HAL driver.
- `app`: Electron/Tauri desktop application, tray icon, settings UI.
- `diagnostics`: Anonymized latency percentiles and schema exports.

### Pull Request Process & Review Checklist

1. **Create a Topic Branch**:
   - `git checkout -b feat/add-arm-neon-optimization` or `git checkout -b fix/pipewire-rebind-leak`.
2. **Adhere to Code Standards**:
   - Ensure `#![forbid(unsafe_code)]` remains intact.
   - Run `cargo fmt` and `cargo clippy`.
3. **Verify Silence Contract**:
   - Verify that your code adheres to the fail-closed silence contract under failure injections.
4. **Pass Offline Gate**:
   - Confirm `./scripts/check-offline.sh` completes cleanly with no network egress.
5. **Open a Pull Request**:
   - Use our [Pull Request Template](.github/pull_request_template.md).
   - Provide a clear summary of changes, rationale, and platform verification results.
6. **Code Review**:
   - At least one core maintainer review is required before merging.
   - All CI gates (Linux, macOS, Windows) must pass green.
   - PRs must be squash-merged or rebased with clean Conventional Commit history.

Thank you for helping build high-performance, privacy-respecting audio software!
