# ClearCore v0.1.0-beta.1 — First Beta Release

We are excited to announce the first official public beta release of **ClearCore (`v0.1.0-beta.1`)**!

ClearCore is a high-performance, cross-platform, realtime AI noise-suppression virtual microphone. Engineered for low latency, zero cloud dependency, and total audio confidentiality, ClearCore runs entirely offline on your local machine using DeepFilterNet3 neural models and optimized native audio engines.

---

## 🚀 Release Highlights

- **DeepFilterNet3 Neural Suppression**: State-of-the-art voice enhancement driven by the DeepFilterNet3 ONNX model, isolating vocals and eliminating keyboard clicks, fan hum, dogs barking, and room reverberation.
- **Ultra-Low Latency Inference**: Native C filter (`libclearcore_filter`) accelerated by Tract with AVX2 instruction optimizations, processing 10ms audio frames in **0.276 ms** (under 3% CPU frame budget).
- **Physical Verification & Acoustics**: Verified in OBS Studio with real hardware audio capture, achieving **-70.3 dBFS** baseline silence during speech pauses and **-32.8 dB** of real-world ambient noise attenuation.
- **First-Party Platform Audio Drivers**:
  - **Linux (PipeWire)**: Native C helper (`pipewire_helper`) registers a per-user `realtime-noise-source` virtual node.
  - **Windows (WaveRT PortCls)**: Dedicated kernel-mode driver (`RealtimeNoise.sys`) paired with a WASAPI host adapter.
  - **macOS (CoreAudio HAL)**: AudioServerPlugIn (`RealtimeNoiseHAL.driver`) paired with a lock-free Swift ring buffer.
- **Resilient Rust Daemon**: Supervisor daemon (`realtime-noise-service`) with automated crash detection, exponential backoff restart policy, and low-latency IPC control.
- **Desktop Companion App**: Turnkey Electron/React application featuring system tray controls, autostart on boot, one-click mode switching (*Active*, *Bypass*, *Mute*), and real-time audio diagnostics.
- **Deterministic Offline Quality Gate**: Automated regression test framework validating processed outputs against Golden Audio Reference vectors prior to packaging.
- **Universal Clean Uninstallation**: Complete teardown suite (`uninstall.sh`, `uninstall.bat`, `uninstall.ps1`, `scripts/uninstall-*`) ensuring zero lingering drivers, daemons, or phantom audio nodes.

---

## 🏗 Architecture Overview

ClearCore operates in a multi-tier pipeline designed for maximum reliability and separation of concerns:

```
[ Physical Microphone ]
          │
          ▼
┌────────────────────────────────────────────────────────┐
│ Platform Audio Capture Bridge                          │
│  - Linux: PipeWire C Helper (pipewire_helper)          │
│  - Windows: WaveRT PortCls Driver (RealtimeNoise.sys)  │
│  - macOS: CoreAudio HAL PlugIn (RealtimeNoiseHAL)      │
└────────────────────────────────────────────────────────┘
          │ (10ms PCM audio frames / 48kHz Float32)
          ▼
┌────────────────────────────────────────────────────────┐
│ Neural Processing Core (libclearcore_filter)           │
│  - Tract AVX2 Inference Engine                         │
│  - DeepFilterNet3 ONNX Model Execution                 │
│  - Latency: 0.276 ms / frame (Sub-millisecond)         │
└────────────────────────────────────────────────────────┘
          │ (Clean voice stream)
          ▼
┌────────────────────────────────────────────────────────┐
│ Virtual Microphone Source Node                         │
│ (realtime-noise-source / Clearcore Virtual Mic)        │
└────────────────────────────────────────────────────────┘
          │
          ▼
[ Target Apps: OBS Studio, Discord, Zoom, Teams, Meet ]

══════════════════════════════════════════════════════════
IPC Supervision:
  [ realtime-noise-service ] ◄──(IPC Bridge)──► [ Electron Tray UI ]
  (Crash recovery, health)                       (Modes, devices, status)
```

---

## 📦 Included Packages & Installers

| Platform / Distro | Architecture | Package Format | Direct Command / Installation |
| :--- | :--- | :--- | :--- |
| **Fedora / RHEL / openSUSE** | x86_64 / AArch64 | Native `.rpm` | `sudo dnf copr enable joaorura/clearcore && sudo dnf install -y clearcore` |
| **Debian / Ubuntu / Mint** | amd64 / arm64 | Native `.deb` | `curl -fsSL https://github.com/joaorura/clearcore/releases/download/v0.1.0-beta.1/Clearcore-0.1.0-beta.1_amd64.deb -o /tmp/clearcore.deb && sudo apt install -y /tmp/clearcore.deb` |
| **Linux (Universal)** | x86_64 / ARM64 | `.AppImage` / `.tar.gz` | `chmod +x Clearcore-*.AppImage && ./Clearcore-*.AppImage` |
| **Windows** | x64 | `.exe` (NSIS / Inno) & `.zip` | `winget install joaorura.Clearcore` (or run `Clearcore-Setup.exe`) |
| **Windows ARM64** | ARM64 (Snapdragon X) | `.zip` | Extract and run `Clearcore.exe` |
| **macOS** | Intel x64 & Apple Silicon | `.dmg` & `.tar.gz` | Open `Clearcore.dmg` or run `./install.sh` |

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
- **Hardware Accelerators (Optional)**: Intel NPU / OpenVINO, AMD Ryzen AI NPU, or ROCm GPU acceleration supported when vendor runtimes are installed.

---

## 📋 Full Changelog for v0.1.0-beta.1

### Added
- Realtime AI Noise Suppression virtual microphone powered by DeepFilterNet3 ONNX.
- Native C library `libclearcore_filter.so` / `clearcore_filter.dll` with Tract AVX2 acceleration (0.276 ms latency per 10ms frame).
- Linux PipeWire C helper (`pipewire_helper`) creating per-user `realtime-noise-source` node.
- Windows WaveRT PortCls kernel driver (`RealtimeNoise.sys`) and WASAPI host adapter.
- macOS CoreAudio HAL AudioServerPlugIn (`RealtimeNoiseHAL.driver`) and lock-free Swift ring buffer.
- Rust supervisor daemon (`realtime-noise-service`) with exponential backoff and IPC control.
- Cross-platform desktop companion application in Electron/React with system tray integration.
- Universal cross-platform uninstaller suite (`uninstall.sh`, `uninstall.bat`, `uninstall.ps1`, `scripts/uninstall-*`) ensuring clean teardown of processes, drivers, services, and audio nodes.
- Offline deterministic qualification gate with Golden Audio Reference verification.

### Changed & Improved
- Resolved PipeWire neural integration and verified physical audio capture in OBS Studio (-70.3 dBFS silence, -32.8 dB noise reduction).
- Portable library discovery in `pipewire_helper.c` via `/proc/self/exe`.
- Localhost TCP bridge on Windows (`127.0.0.1:49215`) for reliable IPC communication without external dependencies.
- Exact process matching (`pkill -x`) in uninstaller scripts to avoid subshell interruption.

---

## 💬 Feedback & Bug Reports

As this is a Beta release, your feedback is crucial in helping us reach General Availability (GA):
- **Issue Tracker**: File bug reports and performance feedback on [GitHub Issues](https://github.com/joaorura/clearcore/issues).
- **Audio Diagnostics**: You can export diagnostic logs directly from the desktop application tray menu -> **Diagnostics**.
- **Security Vulnerabilities**: Please review [`SECURITY.md`](../SECURITY.md) for private reporting procedures.
