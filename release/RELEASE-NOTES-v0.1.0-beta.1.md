# ClearCore v0.1.0-beta.1 — First Beta Release

The first beta of **ClearCore (`v0.1.0-beta.1`)**: a cross-platform, realtime AI noise-suppression virtual microphone with voice isolation by speaker profile.

ClearCore runs entirely offline on your machine: no cloud, no account, no audio leaves the computer. Every package ships the **pDFNet3 Pro** isolation model built in, ready to use with no configuration.

---

## 🚀 Release Highlights

- **pDFNet3 Pro built in**: the isolation model (DeepFilterNet3 with FiLM speaker conditioning) ships inside every package, under `models/pdfnet3/`, and loads by default with no path or environment variable. Its SHA-256 is pinned in the code and checked before loading; if the file is missing or altered, the app says so instead of silently switching to another model. It is the only isolation model shipped.
- **Voice profile (enrollment)**: record a few samples with your microphone, build your voice profile and the virtual microphone isolates your voice from other speakers and background noise. The profile is applied to the same pDFNet3 Pro model the virtual microphone runs.
- **Reliable enrollment**: the noisy-environment check measures the signal-to-noise ratio on the raw recording; the level check measures only active speech (the same rule for samples and for the profile build); stored samples keep weak speech (denoiser attenuation capped at about 12 dB); clear messages for each rejection (noisy environment, too quiet, not enough speech).
- **Build progress you can follow**: a loading state while the profile is built, and a build already running is resumed instead of reported as busy (`GetStatus` exposes `active_enrollment_jobs`).
- **Recording feedback**: smoothed dBFS level meter, low-speech-level warning and the name of the microphone in use (falling back to the system default microphone).
- **Studio DSP chain**: `Natural`, `Podcast`, `Broadcast` and `Off` presets (high-pass, gate/expander, EQ, de-esser, leveler, true-peak limiter).
- **Native virtual microphone**: PipeWire bridge on Linux, WaveRT driver on Windows, CoreAudio HAL on macOS.

---

## 📦 Included Packages & Installers

| Platform / Distro | Architecture | Package Format | Direct Command / Installation |
| :--- | :--- | :--- | :--- |
| **Fedora / RHEL / openSUSE** | x86_64 / AArch64 | Native `.rpm` | `sudo dnf copr enable joaorura/clearcore && sudo dnf install -y clearcore` |
| **Debian / Ubuntu / Mint** | amd64 / arm64 | Native `.deb` | `curl -fsSL https://github.com/joaorura/clearcore/releases/download/v0.1.0-beta.1/Clearcore-0.1.0-beta.1_amd64.deb -o /tmp/clearcore.deb && sudo apt install -y /tmp/clearcore.deb` |
| **Linux (Universal)** | x86_64 / ARM64 | `.AppImage` / `.tar.gz` | `chmod +x Clearcore-*.AppImage && ./Clearcore-*.AppImage` |
| **Windows** | x64 | `.exe` (NSIS / Inno) & `.zip` | `winget install joaorura.Clearcore` (or run `Clearcore-Setup.exe`) |
| **Windows ARM64** | ARM64 (Snapdragon X) | `.zip` | Extract and run `Clearcore.exe` |
| **macOS** | Apple Silicon ARM64 | `.dmg` & `.tar.gz` | Open `Clearcore.dmg` or run `./install.sh` |

---

## 🔒 SHA-256 Integrity Verification

To ensure your downloaded binaries have not been corrupted or tampered with, verify the SHA-256 checksums before installation.

### Verification Commands

- **Linux**:
  ```bash
  sha256sum Clearcore-linux-x64.tar.gz
  ```

- **macOS**:
  ```bash
  shasum -a 256 Clearcore-darwin-arm64.tar.gz
  ```

- **Windows (PowerShell)**:
  ```powershell
  Get-FileHash -Algorithm SHA256 .\Clearcore-win32-x64.zip
  ```

---

## ⚡ Quick Start Guide

### Linux
1. Download and extract the archive:
   ```bash
   tar -xzf Clearcore-linux-x64.tar.gz
   cd Clearcore-linux-x64
   ```
2. Run directly or install to your user environment:
   ```bash
   # Run directly (standalone)
   ./clearcore

   # Or install system menu shortcut and autostart
   ./install.sh
   ```
3. Open your voice application (Discord, Zoom, OBS Studio) and select **"Realtime Noise Virtual Microphone"** (`realtime-noise-source`) as your input device.

### Windows
1. Download and extract **`Clearcore-win32-x64.zip`**.
2. Run **`Clearcore.exe`**:
   - The application automatically launches the companion background daemon (`realtime-noise-service.exe`) and minimizes to the system tray.
   - *SmartScreen*: In this open-source beta, Windows SmartScreen may show an "Unknown Publisher" prompt. Click **"More info"** -> **"Run anyway"**.
3. **Autostart on Boot**:
   ```powershell
   powershell .\resources\scripts\setup-autostart.ps1 -Action enable
   ```
4. **Virtual Microphone Driver (Kernel WaveRT)**:
   - Kernel drivers (`.sys`) in Windows require strict signing. For community beta testing:
     1. Open PowerShell or Command Prompt as **Administrator** and enable test signing:
        ```cmd
        bcdedit /set testsigning on
        ```
     2. Restart your computer.
     3. You can sign or check the driver with the bundled tools:
        ```powershell
        powershell .\resources\scripts\sign-windows-binaries.ps1
        powershell .\resources\scripts\check-virtual-mic-windows.ps1 -Recreate
        ```
   *(Note: The all-in-one `Clearcore-Setup.exe` with EV/WHQL signed driver pipeline is scheduled for General Availability).*

### macOS
1. Download and extract **`Clearcore-darwin-arm64.tar.gz`**.
2. Run the included `install.sh` with administrative privileges to place the CoreAudio HAL driver in `/Library/Audio/Plug-Ins/HAL/`:
   ```bash
   sudo ./install.sh
   ```
3. Launch **`Clearcore.app`** from your Applications folder.

---

## 💻 System Requirements

### Supported Operating Systems
- **Linux**: Ubuntu 22.04 LTS+, Fedora 38+, Arch Linux, Debian 12+, or any system with **PipeWire 0.3+** and **WirePlumber**.
- **Windows**: Windows 10 (build 19041+) or Windows 11 (22H2+) 64-bit.
- **macOS**: macOS 13 (Ventura), macOS 14 (Sonoma), or macOS 15 (Sequoia).

### Hardware Requirements
- **Processor**: 64-bit x86 processor with **AVX2** support (Intel Haswell / AMD Zen or newer), or Apple Silicon (M1/M2/M3/M4 via Rosetta 2 or native HAL).
- **RAM**: Minimum 512 MB free RAM (typical resident set size ~75 MB).
- **Disk Space**: ~250 MB free disk space.
- **CPU**: the pDFNet3 Pro runs on the CPU and requires x86-64 with AVX2. Known limitation of this beta: on ARM64 builds (Linux ARM64, Windows ARM64, macOS Apple Silicon) the isolation model does not load and the app reports `MODEL_LOAD_FAILED`. Hardware accelerators are not used by this release.

---

## 📋 Full Changelog for v0.1.0-beta.1

### Added
- pDFNet3 Pro (FiLM voice-profile conditioning) bundled as the only isolation model: `pdfnet3-release-asset-v1.tar.gz` (daemon, tract) and `pdfnet3-stateful-asset-v1.tar.gz` (virtual microphone, ONNX Runtime), SHA-256 pinned in the code, found automatically in the installation (`/opt/clearcore/models`, `/usr/share/clearcore/models`, next to the executable, `CLEARCORE_MODELS_DIR`). The enrollment model paired with it ships in `models/enrollment/`.
- `GetStatus` reports `isolation_model` (`pdfnet3-pro`, or `none` when unavailable) and `isolation_model_error` (fixed `MODEL_*` code); the profile tab shows the code when the model cannot be loaded.
- Voice enrollment: guided sample recording, profile build with microphone EQ, profile storage with owner-only permissions, voice isolation on/off and intensity.
- Desktop app (Electron/React) with tray controls, level meters, A/B recording test and status telemetry.

### Behavior
- With the pDFNet3 Pro loaded, every backend request runs it on the CPU (tract); requesting an accelerator (OpenVINO, TensorRT) reports a CPU fallback with the reason, since those backends cannot apply a voice profile.
- `voice_profile_supported` is true exactly when the active model can apply a profile; when it is false, "build profile" is refused immediately with a clear message instead of failing at the end.
- Enrollment samples are denoised with the same pDFNet3 Pro in neutral conditioning.
- The base DeepFilterNet3 graphs and TensorRT engines are no longer shipped.


## 💬 Feedback & Bug Reports

As this is a Beta release, your feedback is crucial in helping us reach General Availability (GA):
- **Issue Tracker**: File bug reports and performance feedback on [GitHub Issues](https://github.com/joaorura/clearcore/issues).
- **Audio Diagnostics**: You can export diagnostic logs directly from the desktop application tray menu -> **Diagnostics**.
- **Security Vulnerabilities**: Please review [`SECURITY.md`](../SECURITY.md) for private reporting procedures.
