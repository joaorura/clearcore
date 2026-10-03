# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

ClearCore is a first-party, cross-platform realtime AI noise-suppression virtual microphone powered by DeepFilterNet3 ONNX, Tract AVX2 acceleration, PipeWire C bridge (Linux), WaveRT PortCls kernel driver (Windows), CoreAudio HAL (macOS), and a resilient Rust supervisor daemon.

## [Unreleased]

### Changed
- Licensing: the whole project becomes non-commercial. Source code is now under the PolyForm Noncommercial License 1.0.0; model weights and documentation authored by ClearCore are under CC BY-NC 4.0. Releases up to `v0.1.0-beta.2` stay under the licenses they were published with (Apache-2.0 / MIT OR Apache-2.0). Third-party components keep their own licenses.

## [0.1.0-beta.2] - 2026-10-02

### Fixed
- Linux package: the app now finds `pipewire_helper` in `resources/bin`. In `v0.1.0-beta.1` it looked in a path that is not shipped, so "Recreate virtual mic" silently fell back to a loopback WITHOUT noise suppression while the UI showed Active. Users of `v0.1.0-beta.1` on Linux should reinstall.
- The helper now selects the capture microphone by node name instead of numeric id (WirePlumber reads a numeric `target.object` as `object.serial`, which left the virtual mic with no input).
- Mono microphones (e.g. Bluetooth headsets) are now linked.
- The application icon is installed in the hicolor directory matching its real size.
- The installer prints a hint when GNOME has no AppIndicator extension (no packages are installed automatically).

### Changed
- Hardware accelerator selector: only Auto and CPU (Tract) can be selected. Other accelerators are marked "Preview — does not process audio yet" and the false "backend changed" toast was removed. The OpenVINO/CUDA/NPU backends are not wired to the audio engine in this release; audio is always processed by Tract on CPU.
- Hardware detection now finds OpenVINO installed under `/opt/intel/openvino` or via `INTEL_OPENVINO_DIR` and no longer relies on developer-specific paths.
- Hardware JSON parsing is tolerant to extra output and surfaces detection errors.
- `install-openvino.sh` lists the Fedora packages that exist.
- CI: the nightly hardware-staging workflows are no longer scheduled (no self-hosted runners are registered).
- CI: the release workflow now verifies the Linux package contents and requires per-tag release notes.

### Added
- `studio-dsp` crate: a pure-Rust studio DSP chain (high-pass, EQ, de-esser, compressor, loudness AGC, limiter) with its tests. It is not wired into the audio path yet and has no user-visible effect in this release.

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

[Unreleased]: https://github.com/joaorura/clearcore/compare/v0.1.0-beta.2...HEAD
[0.1.0-beta.2]: https://github.com/joaorura/clearcore/compare/v0.1.0-beta.1...v0.1.0-beta.2
[0.1.0-beta.1]: https://github.com/joaorura/clearcore/releases/tag/v0.1.0-beta.1
