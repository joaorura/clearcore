# Orca / Clearcore - Instructions for LLM Agents

See [`AGENTS.md`](file:///home/joaorura/orca/clearcore/AGENTS.md) for full runbook.

## Core Commands
- **Start All (Daemon + Electron Tray):** `./start-all.sh`
- **Stop All:** `./stop-all.sh`
- **Configure Autostart on Boot (Tray):** `./scripts/setup-autostart.sh [enable|disable|status]`
- **Status (IPC Query):** `cargo run --release -p realtime-noise-app-tauri -- --status`
- **Switch Modes:**
  - Active: `cargo run --release -p realtime-noise-app-tauri -- --mode active`
  - Bypass: `cargo run --release -p realtime-noise-app-tauri -- --mode bypass`
  - Mute: `cargo run --release -p realtime-noise-app-tauri -- --mode mute`
- **Build All:** `ninja -C platform/linux/helper/build && cargo build --release`
- **Offline Gate:** `./scripts/check-offline.sh`
