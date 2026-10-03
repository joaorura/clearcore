# Product Definition: Hippocamp

## Overview
**Hippocamp** is a source-available (non-commercial), local-first cross-platform desktop application (Windows 11, macOS 13+, Ubuntu/Fedora Linux) designed for real-time microphone noise suppression inspired by tools like NVIDIA Broadcast, without sending audio data to external servers.

## Core Vision & Philosophy
1. **100% Local & Privacy-Preserving:** Zero network transmission for audio processing. Full offline capability.
2. **Fail-Closed Safety (Silence-on-Failure):** Under engine crash, pipeline stall, or underrun, the application emits digital silence—never raw, un-suppressed microphone audio—until the pipeline safely recovers.
3. **First-Party Virtual Microphone:** Integrated virtual microphone drivers without requiring third-party tools like VB-Cable, BlackHole, or filter-chain at runtime.
   - **Windows:** Dedicated WaveRT/PortCls audio miniport driver.
   - **macOS:** CoreAudio HAL Audio Server Plug-in.
   - **Linux:** Native PipeWire 1.0+ audio sink/source.
4. **Predictable Low Latency:** Target budget is p95 ≤ 80 ms end-to-end (algorithm + capture ≤ 10 ms + conversion ≤ 5 ms + ring queue ≤ 10 ms + output ≤ 15 ms). CPU inference deadline is 10 ms with p99 ≤ 7 ms.

## Target Audience
- Remote workers, creators, streamers, and gamers across Windows, macOS, and Linux requiring crystal-clear microphone audio on Discord, Microsoft Teams, Zoom, Google Meet, OBS Studio, and browser applications.

## Key Capabilities & Architecture
- **Reference Model:** DeepFilterNet3 (DFN3) full-band audio at 48 kHz mono (`HOP_SAMPLES = 480`).
- **Inference Runtime:** `tract` CPU backend baseline (AVX2-compatible on x86_64, NEON on ARM64/Apple Silicon) with future hardware-accelerated backends (TensorRT, AMD MIGraphX, CoreML).
- **Core Engine & Contracts:** Shared Rust workspace (`crates/contracts`, `crates/model`, `crates/engine`, `crates/supervisor`, `crates/service`).
- **Decoupled User Interface:** Tauri 2 desktop shell communicating with an independent user-session service via IPC. UI closure never interrupts active audio streams.

## Quality & Release Gates
- **Simultaneous GA:** Released simultaneously across Windows, macOS, and Linux.
- **Wave / Milestone Structure:**
  - M0: Asset governance and cryptographic signing (Completed).
  - M1: Reference model, golden verification fixture, and physical benchmark baseline.
  - M2–M6: Audio engine, OS driver/HAL spikes, native production adapters, UI, and hardware accelerators.
