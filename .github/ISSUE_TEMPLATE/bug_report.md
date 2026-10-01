---
name: Bug Report
about: Create a report to help us improve ClearCore realtime noise suppression
title: '[BUG] '
labels: ['bug', 'triage']
assignees: ''
---

## Description
<!-- A clear and concise description of what the bug is. -->

## Environment & Operating System
- **OS**: [e.g. Windows 11 23H2 (Build 22631), macOS 14.5 Sonoma (Apple Silicon), Ubuntu 24.04 LTS, Fedora 42]
- **Architecture**: [e.g. x86_64, arm64]
- **ClearCore Version**: [e.g. 0.1.0 or git commit SHA]
- **App Mode**: [ ] Standalone App Package  [ ] Developer Mode (`./dev.sh`)  [ ] Headless Service Daemon

## Audio Stack & Driver Versions
- **Audio Subsystem**:
  - [ ] **Linux PipeWire**: Output of `pipewire --version` and `wireplumber --version`
  - [ ] **Windows WaveRT / SysVAD**: Driver version displayed in Device Manager
  - [ ] **macOS CoreAudio HAL**: `RealtimeNoiseHAL.driver` version in `/Library/Audio/Plug-Ins/HAL`
- **Physical Audio Hardware**: [e.g. Focusrite Scarlett 2i2, Blue Yeti USB, Built-in Realtek Mic]
- **Sample Rate & Format**: [e.g. 48,000 Hz, 1 channel / 2 channels Float32 or S16LE]

## System Hardware Specifications
- **CPU**: [e.g. Intel Core i5-10210U, AMD Ryzen 7 7840HS, Apple M2 Max]
- **RAM**: [e.g. 16 GB]
- **Accelerator / GPU / NPU** (if applicable): [e.g. None (CPU tract), Intel OpenVINO, NVIDIA TensorRT, AMD Ryzen AI NPU]

## ClearCore Operational State
- **Active Mode at time of issue**:
  - [ ] **Active** (DeepFilterNet3 active noise filtering)
  - [ ] **Bypass** (Pass-through audio without neural inference)
  - [ ] **Mute** (Digital silence)
- **Virtual Microphone Status**:
  <!-- Output of `./scripts/check-virtual-mic.sh --status` (Linux/macOS) or `check-virtual-mic-windows.ps1 -Status` -->
  ```text

  ```

## Steps to Reproduce
1. Start ClearCore via '...'
2. Open target communication app [e.g. Discord, Zoom, OBS, Teams, WebRTC]
3. Select 'ClearCore Virtual Microphone' as the input device
4. Perform action '...'
5. Observe error or unexpected behavior

## Expected Behavior
<!-- A clear and concise description of what you expected to happen. -->

## Actual Behavior
<!-- What actually happened? (e.g. audio dropout, distortion, crash, failure to rebind, daemon restart loop). -->

## Fail-Closed Silence Verification
- **Did the virtual microphone properly fail-closed to digital silence (zeros)?**
  - [ ] **Yes**: No audio was heard (digital silence contract maintained).
  - [ ] **No**: Raw or distorted microphone audio leaked through (**CRITICAL: Please flag immediately**).
  - [ ] **N/A**: Virtual microphone did not initialize or crashed before stream acquisition.

## Sanitized Logs & Diagnostics
<!--
  Run `cargo run --release -p realtime-noise-app-tauri -- --status` or inspect the diagnostics export.
  Ensure ALL personal info (usernames, exact paths, proprietary identifiers) is redacted.
  Never post raw audio files or audio recordings.
-->
<details>
<summary>Diagnostics JSON / Console Log</summary>

```json

```

</details>

## Additional Context & Screenshots
<!-- Add any other context, screenshot of system tray/UI, or notes about the problem here. -->
