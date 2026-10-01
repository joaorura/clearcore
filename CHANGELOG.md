# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

ClearCore is a first-party, cross-platform realtime AI noise-suppression virtual microphone powered by DeepFilterNet3 ONNX, Tract AVX2 acceleration, PipeWire C bridge (Linux), WaveRT PortCls kernel driver (Windows), CoreAudio HAL (macOS), and a resilient Rust supervisor daemon.

## [Unreleased]

## [0.1.0-beta.1] - 2026-10-01

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

### Changed
- Resolved PipeWire neural integration and verified physical audio capture in OBS Studio (-70.3 dBFS silence, -32.8 dB noise reduction).
- Portable library discovery in `pipewire_helper.c` via `/proc/self/exe`.
- Localhost TCP bridge on Windows (`127.0.0.1:49215`) for reliable IPC communication without external dependencies.
- Exact process matching (`pkill -x`) in uninstaller scripts to avoid subshell interruption.

[Unreleased]: https://github.com/joaorura/clearcore/compare/v0.1.0-beta.1...HEAD
[0.1.0-beta.1]: https://github.com/joaorura/clearcore/releases/tag/v0.1.0-beta.1
