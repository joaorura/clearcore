# Sidecar Binaries Convention (`externalBin`)

This directory contains external sidecar binaries packaged alongside the Tauri application.

## Target Triple Suffix Convention

Tauri requires external binaries specified under `bundle.externalBin` to be named with the target triple suffix corresponding to the compilation target:

- Linux x86_64: `realtime-noise-service-x86_64-unknown-linux-gnu`
- Windows x86_64: `realtime-noise-service-x86_64-pc-windows-msvc.exe`
- macOS Apple Silicon: `realtime-noise-service-aarch64-apple-darwin`
- macOS Intel: `realtime-noise-service-x86_64-apple-darwin`

The helper script `npm run prepare:sidecar` automatically builds `realtime-noise-service` from workspace root in release mode and copies it here with the matching target triple.

## Process Lifecycle & Ownership Separation

- **UI Never Owns Audio Processing**: The UI window is strictly a lightweight control and monitoring companion.
- **External Service Lifecycle**: The service (`realtime-noise-service`) runs as an independent per-user daemon / systemd user unit / Windows service registered via `realtime-noise-service --install-user-service`.
- Closing or restarting the UI window does NOT terminate the audio DSP daemon.
- If the service is not currently running, the UI displays connection error states without crashing.
