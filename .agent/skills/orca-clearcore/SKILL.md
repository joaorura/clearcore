---
name: orca-clearcore
description: Realtime noise suppression operations guide for Orca / ClearCore. Use when starting, stopping, querying status, changing audio suppression modes, or compiling ClearCore.
---

# Orca ClearCore Operations

## Quick Commands
- **Start All (Daemon + UI):** `./start-all.sh`
- **Stop All:** `./stop-all.sh`
- **Check Status (IPC):** `cargo run --release -p realtime-noise-app-tauri -- --status`
- **Set Mode Active:** `cargo run --release -p realtime-noise-app-tauri -- --mode active`
- **Set Mode Bypass:** `cargo run --release -p realtime-noise-app-tauri -- --mode bypass`
- **Set Mode Mute:** `cargo run --release -p realtime-noise-app-tauri -- --mode mute`
- **Build All:** `ninja -C platform/linux/helper/build && cargo build --release`
- **Offline Gate:** `./scripts/check-offline.sh`
