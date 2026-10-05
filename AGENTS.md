# Orca / Clearcore - Realtime Noise Suppression

## Overview
First-party realtime noise-suppression virtual microphone powered by DeepFilterNet3 ONNX, PipeWire C bridge (Linux), WaveRT PortCls driver (Windows), CoreAudio HAL (macOS), and Rust supervisor daemon.

## Package Managers & Distribution
- **Fedora / RHEL (DNF via Copr):**
  `sudo dnf copr enable joaorura/clearcore && sudo dnf install -y clearcore`
- **Windows (WinGet):**
  `winget install joaorura.Clearcore`
- **Ubuntu / Debian (APT):**
  - *Method 1 (Official One-Liner - Recommended):*  
    `curl -fsSL https://github.com/joaorura/clearcore/releases/download/v0.1.0-beta.1/Clearcore-0.1.0-beta.1_amd64.deb -o /tmp/clearcore.deb && sudo apt install -y /tmp/clearcore.deb && rm /tmp/clearcore.deb`
  - *Method 2 (Modular GitHub Pages APT - Solution A):*  
    `echo "deb [trusted=yes] https://joaorura.github.io/clearcore/apt stable main" | sudo tee /etc/apt/sources.list.d/clearcore.list && sudo apt update && sudo apt install -y clearcore`

## Standalone Application & Packaging (Turnkey Distribution)

The desktop application is completely self-contained. When launched, the application automatically starts its companion daemon (`realtime-noise-service`) in the background if not already running, verifies/creates the virtual microphone, and minimizes to the system tray. Exiting via the tray cleanly stops the daemon.

### Build & Package Standalone Installers
- **Build & Package Everything (Linux / macOS):** `./package.sh`
- **Build & Package Everything (Windows):** `.\package.bat` ou `.\package.ps1`
- **Output:**
  - Linux: `release/Clearcore-linux-x64/` (run `./clearcore` directly or `./install.sh` for system menu integration) + `Clearcore-linux-x64.tar.gz`
  - Windows: `release/Clearcore-win32-x64/` (portable `Clearcore.exe`) + NSIS (`Clearcore-Setup.nsi`) and Inno Setup (`Clearcore-Setup.iss`) to produce single-click `Clearcore-Setup.exe` that installs the app and WaveRT audio driver automatically.
  - macOS: `release/Clearcore-darwin-x64/` (`Clearcore.app` with embedded CoreAudio HAL driver bundle) + `install.sh` + `.tar.gz`

## Quick Commands

### Development Mode (Vite HMR + Electron + Sidecar Daemon)
To run the full development environment with live React hot reloading and automatic backend daemon:
- **Linux & macOS:** `./dev.sh` (or `cd crates/app-tauri && npm run dev`)
- **Windows:** `.\dev.bat` ou `.\dev.ps1` (or `cd crates/app-tauri && npm run dev`)
- **React Frontend Only (Browser):** `cd crates/app-tauri && npm run dev:ui`

### Standalone Desktop App Execution (Production Package)
- **Run Standalone App (Linux):** `./release/Clearcore-linux-x64/clearcore`
- **Run Standalone App (Windows):** `.\release\Clearcore-win32-x64\Clearcore.exe`
- **Run Standalone App (macOS):** `open ./release/Clearcore-darwin-x64/Clearcore.app`

### Developer Convenience & Lifecycle

#### Linux & macOS
- **Start Everything (Daemon + Electron Tray):** `./start-all.sh`
- **Stop Everything:** `./stop-all.sh`
- **Check Virtual Mic:** `./scripts/check-virtual-mic.sh [--status|--json|--recreate|--set-default]`
- **Configure Autostart on Boot (Tray):** `./scripts/setup-autostart.sh [enable|disable|status]`

#### Windows (CMD & PowerShell)
- **Start Everything (Daemon + Electron Tray):** `start-all.bat` ou `.\start-all.ps1`
- **Stop Everything:** `stop-all.bat` ou `.\stop-all.ps1`
- **Check Virtual Mic:** `powershell .\scripts\check-virtual-mic-windows.ps1 [-Status|-Json|-Recreate|-SetDefault]`
- **Configure Autostart on Boot (Tray):** `.\scripts\setup-autostart.bat [enable|disable|status]` ou `powershell .\scripts\setup-autostart.ps1 -Action [enable|disable|status]`

#### IPC Status & Modes (Cross-Platform)
- **Check Status (IPC):** `cargo run --release -p realtime-noise-app-tauri -- --status`
- **Active Mode (DeepFilterNet):** `cargo run --release -p realtime-noise-app-tauri -- --mode active`
- **Bypass Mode:** `cargo run --release -p realtime-noise-app-tauri -- --mode bypass`
- **Mute Mode:** `cargo run --release -p realtime-noise-app-tauri -- --mode mute`

### Build & Test
- **Package Desktop Application:** `./package.sh`
- **Build PipeWire Helper (Linux):** `ninja -C platform/linux/helper/build`
- **Build Rust Service:** `cargo build --release -p realtime-noise-service`
- **Build All:** `cargo build --release`
- **Run Offline Gate:** `./scripts/check-offline.sh`

## Orca IDE Integration
Tasks are configured in `.vscode/tasks.json` and can be triggered directly via `Ctrl+Shift+B` or Command Palette (`Tasks: Run Task`).
Debug and Launch profiles are located in `.vscode/launch.json`.
