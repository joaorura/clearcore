# ClearCore v0.1.0-beta.2 — Beta Fixes Release

This release fixes voice profile enrollment and the error reporting of the desktop application in **ClearCore (`v0.1.0-beta.2`)**.

ClearCore is a high-performance, cross-platform, realtime AI noise-suppression virtual microphone. Engineered for low latency, zero cloud dependency, and total audio confidentiality, ClearCore runs entirely offline on your local machine using DeepFilterNet3 neural models and optimized native audio engines.

---

## 🚀 Release Highlights

- **Voice profile enrollment fixed**: recording a voice profile no longer fails on every recording. The noisy-environment check now measures the signal-to-noise ratio on the raw recording, so the speech removed by the denoiser is no longer counted as noise.
- **Clear noisy-environment message**: when a recording is rejected for background noise, the application now says so, instead of reporting that the service could not process the audio.

---

## 📦 Included Packages & Installers

| Platform / Distro | Architecture | Package Format | Direct Command / Installation |
| :--- | :--- | :--- | :--- |
| **Fedora / RHEL / openSUSE** | x86_64 / AArch64 | Native `.rpm` | `sudo dnf copr enable joaorura/clearcore && sudo dnf install -y clearcore` |
| **Debian / Ubuntu / Mint** | amd64 / arm64 | Native `.deb` | `curl -fsSL https://github.com/joaorura/clearcore/releases/download/v0.1.0-beta.2/Clearcore-0.1.0-beta.2_amd64.deb -o /tmp/clearcore.deb && sudo apt install -y /tmp/clearcore.deb` |
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
- **Hardware Accelerators (Optional)**: Intel NPU / OpenVINO, AMD Ryzen AI NPU, or ROCm GPU acceleration supported when vendor runtimes are installed.

---

## 📋 Full Changelog for v0.1.0-beta.2

### Fixed
- Voice profile enrollment failed on every recording: the noisy-environment gate estimated the noise as the raw-minus-denoised difference, which counted the voice itself removed by the denoiser as noise. The gate now estimates the SNR on the raw signal and logs `snr_db`.
- The `ENROLL_NOISY_ENVIRONMENT` code was not recognized by the Electron layer and was shown as "O serviço não conseguiu processar o áudio". It now shows the correct noisy-environment message.

---

## 💬 Feedback & Bug Reports

As this is a Beta release, your feedback is crucial in helping us reach General Availability (GA):
- **Issue Tracker**: File bug reports and performance feedback on [GitHub Issues](https://github.com/joaorura/clearcore/issues).
- **Audio Diagnostics**: You can export diagnostic logs directly from the desktop application tray menu -> **Diagnostics**.
- **Security Vulnerabilities**: Please review [`SECURITY.md`](../SECURITY.md) for private reporting procedures.
