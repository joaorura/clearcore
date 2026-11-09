# Orca / Clearcore - Instructions for LLM Agents

See [`AGENTS.md`](AGENTS.md) for full runbook.

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

## Build & Packaging (Standalone Installers)
- **Package Everything (Linux/macOS):** `./package.sh`
- **Package Everything (Windows):** `.\package.bat` ou `.\package.ps1`
- **Run Packaged App (Linux):** `./release/Clearcore-linux-x64/clearcore`
- **Run Packaged App (Windows):** `.\release\Clearcore-win32-x64\Clearcore.exe`
- **Run Packaged App (macOS):** `open ./release/Clearcore-darwin-x64/Clearcore.app`

## Development Mode (HMR + Electron + Sidecar Daemon)
- **Linux & macOS:** `./dev.sh` (or `cd crates/app-tauri && npm run dev`)
- **Windows:** `.\dev.bat` ou `.\dev.ps1` (or `cd crates/app-tauri && npm run dev`)
- **UI Only (Browser):** `cd crates/app-tauri && npm run dev:ui`

## Core Commands

### Linux & macOS
- **Start All (Daemon + Electron Tray):** `./start-all.sh`
- **Stop All:** `./stop-all.sh`
- **Virtual Mic Verification/Creation:** `./scripts/check-virtual-mic.sh [--status|--json|--recreate|--set-default]`
- **Configure Autostart on Boot (Tray):** `./scripts/setup-autostart.sh [enable|disable|status]`

### Windows
- **Start All (Daemon + Electron Tray):** `start-all.bat` ou `.\start-all.ps1`
- **Stop All:** `stop-all.bat` ou `.\stop-all.ps1`
- **Virtual Mic Verification/Creation:** `powershell .\scripts\check-virtual-mic-windows.ps1 [-Status|-Json|-Recreate|-SetDefault]`
- **Configure Autostart on Boot (Tray):** `.\scripts\setup-autostart.bat [enable|disable|status]`

### IPC Control & Modes (All Platforms)
- **Status (IPC Query):** `cargo run --release -p realtime-noise-app-tauri -- --status`
- **Switch Modes:**
  - Active: `cargo run --release -p realtime-noise-app-tauri -- --mode active`
  - Bypass: `cargo run --release -p realtime-noise-app-tauri -- --mode bypass`
  - Mute: `cargo run --release -p realtime-noise-app-tauri -- --mode mute`
- **Build All:** `cargo build --release`
- **Offline Gate:** `./scripts/check-offline.sh`
