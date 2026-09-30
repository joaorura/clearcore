# Technology Stack: Hippocamp

## 1. Core & Architecture
- **Language:** Rust (Edition 2024, toolchain pinned at `1.90.0`).
- **Workspace Policy:** Strict compile-time checks (`unsafe_code = "forbid"`, Clippy pedantic/nursery warnings, zero unwrap/panic in core libraries).
- **Core Crates:**
  - `crates/contracts`: Audio framing, ring buffers, wire ABI (`WireFrameEnvelopeV1`).
  - `crates/model`: Tract backend abstraction, asset manifests, golden fixture verification.
  - `crates/tools`: Offline benchmark, hardware observation, golden generator, asset validation.
  - `crates/engine`: Low-latency audio processing loop, silence-on-failure state machine.
  - `crates/supervisor`: Process watchdog, backoff crash recovery, health telemetry.
  - `crates/service`: Background OS user session service.

## 2. Neural Audio & DSP
- **Default Reference Model:** DeepFilterNet3 (DFN3 `v0.5.6`) full-band (48 kHz mono, `HOP_SAMPLES = 480`).
- **Inference Runtime:** `tract` CPU (AVX2 on x86_64, NEON on ARM64/Apple Silicon) with `default-features = false`.
- **Planned Accelerators:** NVIDIA TensorRT, AMD MIGraphX, Apple CoreML, Intel OpenVINO.

## 3. Platform Native Virtual Drivers
- **Windows 11:** C++20 with Windows Driver Kit (WDK) PortCls / WaveRT virtual miniport driver.
- **macOS 13+:** Swift + C with CoreAudio HAL Audio Server Plug-in.
- **Linux (Ubuntu 24.04+, Fedora 42+):** C17 with native PipeWire 1.0+ filter/node integration.

## 4. User Interface & Presentation
- **Framework:** Tauri 2 desktop shell.
- **Frontend:** TypeScript + React + Vite + Tailwind CSS.
- **Decoupled Architecture:** GUI runs as a client communicating via local IPC (`realtime-noise.v1`) with the background daemon. Closing or crashing the UI never drops or halts audio streaming.

## 5. Quality, Verification & Tooling
- **CI/CD:** Multi-stage GitHub Actions, Docker-isolated offline execution (`hippocamp-task4-rust:local`).
- **Verification Harness:** Golden reference comparator (`generate-golden`), sustained frequency measurement, allocation tracking via `stats_alloc`.
- **Supply Chain:** SPDX Software Bill of Materials (SBOM), pinned dependencies with locked revisions.
