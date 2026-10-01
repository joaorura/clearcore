# Orca / Clearcore - Realtime Noise Suppression

## Overview
First-party realtime noise-suppression virtual microphone powered by DeepFilterNet3 ONNX, PipeWire C bridge, and Rust supervisor daemon.

## Quick Commands

### Startup & Lifecycle
- **Start Everything (Daemon + Electron Tray):** `./start-all.sh`
- **Stop Everything:** `./stop-all.sh`
- **Start Daemon Only:** `./start-realtime-noise.sh`
- **Stop Daemon Only:** `./stop-realtime-noise.sh`
- **Configure Autostart on Boot (Tray):** `./scripts/setup-autostart.sh [enable|disable|status]`
- **Check Status (IPC):** `cargo run --release -p realtime-noise-app-tauri -- --status`

### Audio Denoise Modes
- **Active Mode (DeepFilterNet):** `cargo run --release -p realtime-noise-app-tauri -- --mode active`
- **Bypass Mode:** `cargo run --release -p realtime-noise-app-tauri -- --mode bypass`
- **Mute Mode:** `cargo run --release -p realtime-noise-app-tauri -- --mode mute`

### Build & Test
- **Build PipeWire Helper:** `ninja -C platform/linux/helper/build`
- **Build Rust Service:** `cargo build --release -p realtime-noise-service`
- **Build All:** `ninja -C platform/linux/helper/build && cargo build --release`
- **Run Offline Gate:** `./scripts/check-offline.sh`

## Orca IDE Integration
Tasks are configured in `.vscode/tasks.json` and can be triggered directly via `Ctrl+Shift+B` or Command Palette (`Tasks: Run Task`).
Debug and Launch profiles are located in `.vscode/launch.json`.
