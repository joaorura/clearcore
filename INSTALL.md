# ClearCore Installation & Setup Guide

This guide covers installing, configuring, verifying, and troubleshooting **ClearCore** across Linux, macOS, and Windows.

ClearCore provides three flexible installation methods on every supported platform:
1. **Pre-Packaged Turnkey Binary Installer**: System-wide or user-profile integration with desktop menu entries, icon assets, and driver registration.
2. **Portable Standalone Execution**: Direct zero-installation executable for testing or running without root/administrator privileges.
3. **Building from Source**: For developers, contributors, and custom CI/CD pipelines.

---

## Table of Contents

- [System Requirements & Support Matrix](#system-requirements--support-matrix)
- [Linux Installation (Ubuntu, Debian, Fedora, Arch)](#linux-installation)
  - [Path A: Turnkey Binary Installer](#linux-path-a-turnkey-binary-installer)
  - [Path B: Portable Standalone Execution](#linux-path-b-portable-standalone-execution)
  - [Path C: Building from Source](#linux-path-c-building-from-source)
- [macOS Installation (Apple Silicon & Intel)](#macos-installation)
  - [Path A: Turnkey Application Installer](#macos-path-a-turnkey-application-installer)
  - [Path B: Portable Standalone Execution](#macos-path-b-portable-standalone-execution)
  - [Path C: Building from Source](#macos-path-c-building-from-source)
- [Windows Installation (Windows 10 & 11 x64)](#windows-installation)
  - [Path A: Turnkey Single-Click Setup (`Clearcore-Setup.exe`)](#windows-path-a-turnkey-single-click-setup)
  - [Path B: Portable Standalone Execution](#windows-path-b-portable-standalone-execution)
  - [Path C: Building from Source](#windows-path-c-building-from-source)
- [Virtual Microphone Verification Procedures](#virtual-microphone-verification-procedures)
  - [CLI & Diagnostics Verification](#cli--diagnostics-verification)
  - [Application Setup (OBS, Discord, Zoom, Teams, WebRTC)](#application-setup)
- [Autostart on Boot Configuration](#autostart-on-boot-configuration)
- [Uninstallation & Clean Removal](#uninstallation--clean-removal)
- [Troubleshooting & FAQ](#troubleshooting--faq)

---

## System Requirements & Support Matrix

| Parameter | Specification | Notes |
|---|---|---|
| **CPU Architecture** | x86_64 with **AVX2** support, or ARM64 (Apple Silicon M1–M4) | AVX2 accelerates DeepFilterNet3 neural inference (<0.3 ms/frame) |
| **RAM** | 4 GB Minimum (Daemon consumes < 60 MB RAM) | Real-time ring buffers preallocated at startup |
| **Linux OS** | Ubuntu 22.04 LTS+, Fedora 38+, Arch Linux, Debian 12+ | PipeWire 0.3.50+ or 1.0+ with WirePlumber 0.4.10+ |
| **macOS** | macOS 12 (Monterey), 13 (Ventura), 14 (Sonoma), 15 (Sequoia) | Intel Core or Apple Silicon (Universal / Native) |
| **Windows OS** | Windows 10 (version 1903+) or Windows 11 (21H2/22H2/23H2/24H2) x64 | WaveRT audio architecture supported |

> [!NOTE]
> To check if your Linux CPU supports AVX2:
> ```bash
> grep -q avx2 /proc/cpuinfo && echo "✅ AVX2 Supported" || echo "❌ AVX2 Not Detected"
> ```

---

## Linux Installation

ClearCore integrates natively with **PipeWire** on Linux, creating an isolated `realtime-noise-source` node (`Audio/Source`) with zero heap allocations in the real-time processing callback.

### Linux Path A: Turnkey Binary Installer

Download or unpack the official release archive `Clearcore-linux-x64.tar.gz`:

```bash
# 1. Extract the release package
tar -xzf Clearcore-linux-x64.tar.gz
cd Clearcore-linux-x64

# 2. Run the installer script
# For User-Local installation (no sudo required, installs to ~/.local/share/clearcore):
./install.sh

# OR for System-Wide installation (installs to /opt/clearcore and /usr/local/bin):
sudo ./install.sh
```

**What the installer performs:**
1. Copies runtime binaries (`clearcore`, `realtime-noise-service`, `pipewire_helper`, `libclearcore_filter.so`).
2. Symlinks launcher to `~/.local/bin/clearcore` (or `/usr/local/bin/clearcore`).
3. Installs high-resolution icons to the desktop environment icon cache.
4. Registers `clearcore.desktop` into your desktop application launcher menu.

Launch ClearCore anytime from your application launcher or by running:
```bash
clearcore
```

---

### Linux Path B: Portable Standalone Execution

If you prefer not to touch system directories or application menus, you can run ClearCore completely portably:

```bash
cd Clearcore-linux-x64
./clearcore
```

- When launched, ClearCore automatically detects if the background service (`realtime-noise-service`) is running; if not, it spawns its companion daemon automatically.
- The virtual microphone node `realtime-noise-source` is registered in the PipeWire graph instantly.
- Exiting via the system tray or closing the app cleanly stops the helper and releases all audio resources.

---

### Linux Path C: Building from Source

#### 1. Install Build Dependencies
- **Ubuntu / Debian:**
  ```bash
  sudo apt update
  sudo apt install -y build-essential pkg-config libpipewire-0.3-dev libspa-0.2-dev \
                      clang meson ninja-build curl git
  ```
- **Fedora / RHEL:**
  ```bash
  sudo dnf install -y gcc gcc-c++ pipewire-devel libspa-devel clang meson ninja-build git
  ```
- **Arch Linux:**
  ```bash
  sudo pacman -S --needed base-devel pipewire pipewire-pulse clang meson ninja git
  ```

#### 2. Install Rust (1.90.0+) and Node.js
```bash
# Install Rust toolchain
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain 1.90.0
source "$HOME/.cargo/env"

# Install Node.js (v20+) & npm
curl -fsSL https://deb.nodesource.com/setup_20.x | sudo -E bash -
sudo apt install -y nodejs
```

#### 3. Clone and Build ClearCore
```bash
git clone https://github.com/clearcore/clearcore.git
cd clearcore

# Run offline qualification gate
./scripts/check-offline.sh

# Build all Rust crates in release mode
cargo build --release

# Package desktop application and native helpers
./package.sh
```

The resulting standalone distribution will be generated in `release/Clearcore-linux-x64/`.

---

## macOS Installation

On macOS, ClearCore uses a dedicated **CoreAudio HAL AudioServerPlugIn** (`RealtimeNoiseHAL.driver`) to deliver a true hardware-like virtual microphone to macOS applications.

### macOS Path A: Turnkey Application Installer

1. Download and unpack `Clearcore-darwin-x64.tar.gz` (or open the DMG/PKG installer).
2. Open terminal in the uncompressed folder and run:
   ```bash
   ./install.sh
   ```
   Or manually drag `Clearcore.app` to your `/Applications` directory.
3. The installer registers `RealtimeNoiseHAL.driver` into `/Library/Audio/Plug-Ins/HAL/` (or `~/Library/Audio/Plug-Ins/HAL/`) and restarts `coreaudiod` so macOS registers the device immediately without needing a system reboot.
4. Launch **ClearCore** from Launchpad or `/Applications/Clearcore.app`.

> [!IMPORTANT]
> When opening for the first time, macOS Gatekeeper may prompt for approval. Navigate to **System Settings** -> **Privacy & Security** -> **Microphone** and ensure ClearCore is granted microphone access.

---

### macOS Path B: Portable Standalone Execution

Run the portable app directly from terminal or Finder:
```bash
open ./release/Clearcore-darwin-x64/Clearcore.app
```
To run the background noise suppression daemon directly in CLI mode:
```bash
cargo run --release -p realtime-noise-service
```

---

### macOS Path C: Building from Source

#### 1. Prerequisites
- macOS 12 Monterey or newer.
- Xcode Command Line Tools: `xcode-select --install`
- Rust 1.90+: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
- Node.js 20+: `brew install node`

#### 2. Build & Package
```bash
git clone https://github.com/clearcore/clearcore.git
cd clearcore

# Build Rust crates
cargo build --release

# Package macOS App bundle
./package.sh
```
The output `Clearcore.app` is placed in `release/Clearcore-darwin-x64/`.

---

## Windows Installation

On Windows, ClearCore integrates via a high-performance **WaveRT PortCls virtual audio streaming driver** (`RealtimeNoise.inf`), providing sub-millisecond audio streaming compatible with WASAPI exclusive and shared modes.

### Windows Path A: Turnkey Single-Click Setup

1. Download and run `Clearcore-Setup.exe` (or run Inno Setup / NSIS generated installer).
2. The setup wizard:
   - Installs ClearCore into `C:\Program Files\Clearcore\`.
   - Stages and installs the `RealtimeNoise.inf` WaveRT audio driver using `pnputil.exe /add-driver RealtimeNoise.inf /install`.
   - Configures the `RealtimeNoiseService` background worker.
   - Places shortcuts in the Start Menu and Desktop.
3. Complete the wizard and launch ClearCore. The virtual microphone **Realtime Noise Virtual Microphone** is now available system-wide.

---

### Windows Path B: Portable Standalone Execution

Extract the portable release zip `Clearcore-win32-x64.zip` and run:
```powershell
.\release\Clearcore-win32-x64\Clearcore.exe
```
Or start all components via the helper script:
```powershell
.\start-all.ps1
```

---

### Windows Path C: Building from Source

#### 1. Prerequisites
- **Visual Studio 2022** with "Desktop development with C++" workload installed.
- **Windows Driver Kit (WDK)** for Windows 10/11 (for WaveRT driver compilation).
- **Rust 1.90+**: Install via [rustup.rs](https://rustup.rs/) (MSVC toolchain: `x86_64-pc-windows-msvc`).
- **Node.js 20+**: Install via [nodejs.org](https://nodejs.org/).
- **PowerShell 7+** recommended.

#### 2. Compilation and Packaging
```powershell
git clone https://github.com/clearcore/clearcore.git
cd clearcore

# Build all workspace crates
cargo build --release

# Package Electron desktop app and Windows bundles
.\package.ps1
```

---

## Virtual Microphone Verification Procedures

### CLI & Diagnostics Verification

You can instantly check if the virtual microphone is created, healthy, and operational:

#### Linux
```bash
# Fast check (exit code 0 = active, 1 = inactive)
./scripts/check-virtual-mic.sh --check-only

# Formatted status output
./scripts/check-virtual-mic.sh --status

# Machine-readable JSON output
./scripts/check-virtual-mic.sh --json

# Query supervisor daemon state via IPC
cargo run --release -p realtime-noise-app-tauri -- --status
```

#### macOS
```bash
./scripts/check-virtual-mic-macos.sh --status
```

#### Windows (PowerShell)
```powershell
powershell .\scripts\check-virtual-mic-windows.ps1 -Status
powershell .\scripts\check-virtual-mic-windows.ps1 -Json
```

---

### Application Setup

Once ClearCore is running, configure your favorite communication and recording tools to use the virtual microphone.

#### 1. OBS Studio
1. Open **OBS Studio** -> **Settings** -> **Audio**.
2. Under **Global Audio Devices**, set **Mic/Auxiliary Audio** to:
   - **Linux:** `Realtime Noise Virtual Microphone` (or `realtime-noise-source`).
   - **macOS:** `Realtime Noise Virtual Microphone`.
   - **Windows:** `Realtime Noise Virtual Microphone (WaveRT)`.
3. In the OBS Audio Mixer, verify speech levels bounce cleanly while typing, fan noise, or background chatter remain completely silent.

#### 2. Discord
1. Go to **User Settings (gear icon)** -> **Voice & Video**.
2. Set **Input Device** to `Realtime Noise Virtual Microphone`.
3. Under **Voice Processing**:
   - Turn **Noise Suppression** to **None** (disable Krisp and Standard). ClearCore's DeepFilterNet3 neural engine already suppresses noise at -32.8 dB reduction; disabling redundant DSP prevents double-filtering distortion.
   - Disable **Echo Cancellation** and **Noise Reduction** if you experience voice clipping.

#### 3. Zoom Meetings
1. Open **Settings** -> **Audio**.
2. Under **Microphone**, select `Realtime Noise Virtual Microphone`.
3. Set **Suppress background noise** to **Low** or **Off** (allow ClearCore to do the heavy lifting).
4. Uncheck "Automatically adjust microphone volume" for best acoustic consistency.

#### 4. Microsoft Teams
1. Click **Settings and more (...)** -> **Settings** -> **Devices**.
2. Under **Audio devices** -> **Microphone**, select `Realtime Noise Virtual Microphone`.
3. Set **Noise suppression** to **Off**.

#### 5. WebRTC & Web Browsers (Chrome / Firefox / Edge)
1. Open any WebRTC audio testing site (e.g., [WebcamMicTest](https://webcamtests.com/mic) or [OnlineVoiceRecorder](https://online-voice-recorder.com/)).
2. Grant microphone access and choose `Realtime Noise Virtual Microphone` from the browser device picker.
3. Record a sample while clapping or typing on a mechanical keyboard to test noise cancellation clarity.

---

## Autostart on Boot Configuration

Configure ClearCore to start silently minimized to your system tray on computer startup:

### Linux
```bash
# Enable autostart on user login:
./scripts/setup-autostart.sh enable

# Check autostart status:
./scripts/setup-autostart.sh status

# Disable autostart:
./scripts/setup-autostart.sh disable
```

### Windows
```cmd
:: CMD:
.\scripts\setup-autostart.bat enable
.\scripts\setup-autostart.bat status
.\scripts\setup-autostart.bat disable
```
Or via PowerShell:
```powershell
powershell .\scripts\setup-autostart.ps1 -Action enable
powershell .\scripts\setup-autostart.ps1 -Action status
powershell .\scripts\setup-autostart.ps1 -Action disable
```

---

## Uninstallation & Clean Removal

ClearCore strictly complies with isolated component lifecycle management. Uninstalling ClearCore leaves physical audio endpoints, system drivers, and third-party configuration 100% untouched.

### Universal Uninstaller (Linux / macOS / Windows)

In the repository root or release bundle, run:

- **Linux & macOS:**
  ```bash
  # Standalone or User uninstallation:
  ./uninstall.sh

  # If installed system-wide (e.g. to /opt/clearcore):
  sudo ./uninstall.sh
  ```

- **Windows (Command Prompt / PowerShell):**
  ```cmd
  uninstall.bat
  ```
  Or:
  ```powershell
  powershell -NoProfile -ExecutionPolicy Bypass -File .\uninstall.ps1
  ```

### What Uninstallation Cleans Up:
- **Processes:** Terminates all active instances of `clearcore`, `realtime-noise-service`, and `pipewire_helper`.
- **Background Units:** Removes systemd user units (`realtime-noise.service`) or macOS LaunchAgents (`com.clearcore.realtime-noise.plist`).
- **Drivers & Nodes:** Destroys the PipeWire source node, unloads `RealtimeNoiseHAL.driver` from CoreAudio, or removes `RealtimeNoise.inf` via `pnputil.exe /delete-driver`.
- **Files & Menus:** Deletes desktop shortcuts, application icons, runtime sockets (`/tmp/realtime-noise.sock`), and lockfiles.

For advanced uninstallation details, see [`docs/uninstall.md`](docs/uninstall.md).

---

## Troubleshooting & FAQ

### 1. The Virtual Microphone does not appear in audio settings
- **Linux:** Ensure PipeWire and WirePlumber are active:
  ```bash
  systemctl --user status pipewire wireplumber
  ./scripts/check-virtual-mic.sh --recreate
  ```
- **macOS:** Restart the CoreAudio daemon:
  ```bash
  sudo killall coreaudiod
  ./scripts/check-virtual-mic-macos.sh --recreate
  ```
- **Windows:** Check if the driver is installed:
  ```powershell
  pnputil /enum-drivers | Select-String "RealtimeNoise"
  powershell .\scripts\check-virtual-mic-windows.ps1 -Recreate
  ```

### 2. Audio is silent (Digital Silence Output)
ClearCore adheres to a **Fail-Closed Digital Silence Policy**. If the neural engine experiences an error, unready state, or disconnect, it outputs pure digital silence instead of leaking raw ambient noise:
- Check the supervisor status:
  ```bash
  cargo run --release -p realtime-noise-app-tauri -- --status
  ```
- If the status is `EngineUnavailable` or `TerminalSafeState`, request a generation restart:
  ```bash
  cargo run --release -p realtime-noise-app-tauri -- --restart
  ```
- Ensure your physical input microphone is connected and not muted.

### 3. Audio Crackling, Dropouts, or Quantum Mismatch (Linux)
ClearCore operates natively at **48,000 Hz, 480 samples per quantum (10 ms)**. If another application forces an incompatible quantum:
```bash
# Force PipeWire 48 kHz quantum:
pw-metadata -n settings 0 clock.force-rate 48000
pw-metadata -n settings 0 clock.force-quantum 480
```

### 4. Device Busy / UnavailableBusy (Exit Code 2)
If an old daemon or orphaned helper holds the IPC socket or lock:
```bash
./stop-all.sh
# Check if any zombie process persists:
pkill -9 pipewire_helper
pkill -9 realtime-noise-service
./start-all.sh
```

---

Need additional help? Open an issue on GitHub or consult [`docs/service-lifecycle.md`](docs/service-lifecycle.md) and [`docs/ipc-v1.md`](docs/ipc-v1.md).
