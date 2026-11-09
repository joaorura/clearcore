# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

ClearCore is a first-party, cross-platform realtime AI noise-suppression virtual microphone powered by DeepFilterNet3 ONNX, ONNX Runtime (`ort` v2) neural streaming, Target Speaker Extraction (DFNet-TSE-MHCA), PipeWire C bridge (Linux), WaveRT PortCls kernel driver (Windows), CoreAudio HAL (macOS), and a resilient Rust supervisor daemon.

## [Unreleased]

### Added
- **pDFNet3 Pro on OpenVINO (Intel NPU, GPU, CPU)**: selecting `openvino-npu`, `openvino-gpu`, `openvino-cpu` or `openvino` runs the bundled pDFNet3 Pro stateful export on OpenVINO with static shapes, the FiLM `gamma`/`beta` inputs fed (identity without a profile) and the profile EQ applied like tract; `voice_profile_supported` stays `true`. Any failure (runtime, plugin, device, compilation, real-time qualification) installs tract and reports the reason. `auto` stays on tract (see `docs/openvino.md`).
- Linux x64 and Windows x64 packages embed the OpenVINO 2026.4.1 runtime (CPU/GPU/NPU plugins, from the PyPI wheel pinned by SHA-256 in `packaging/openvino/openvino-runtime.lock`) under `resources/openvino` (+147 MiB unpacked, +49 MB in the tar.gz).
- `GetStatus`: `openvino_runtime` (`status`: `ok`, `not_found`, `incompatible_version`, `load_failed`, `npu_plugin_missing`, `npu_device_unavailable`; version, library, devices) and `backend_fallback` / `backend_fallback_reason`.

### Changed
- The OpenVINO loader accepts the 2025 and 2026 series (C ABI checked against the 2025.4 and 2026.4 headers and the exported symbols of 2025.4, 2026.0 and 2026.4) and finds `libopenvino_c.so*` by glob in `OPENVINO_LIB_PATH`, the embedded runtime, `/usr/lib64`, `/lib64` and `/usr/local/lib64`.
- `scripts/install-openvino.sh` only diagnoses and lists the NPU/GPU driver packages that exist on each distribution; `detect-hardware` recognizes the embedded runtime.

## [0.1.0-beta.1] - 2026-11-09

First beta release.

### Added
- **pDFNet3 Pro built in**: the isolation model (DeepFilterNet3 with FiLM speaker conditioning, checkpoint `pro-v2-mhca-k16-tse`, export `m3_deploy_pro_v3_20261013`) ships in every package under `models/pdfnet3/` and is loaded by default, with no path or environment variable: `pdfnet3-release-asset-v1.tar.gz` (daemon, tract) and `pdfnet3-stateful-asset-v1.tar.gz` (virtual microphone, ONNX Runtime). Both whole-file SHA-256 are pinned in `crates/model/src/pdfnet3_bundle.rs` and checked before loading (also by the package build and the release workflow). It is the only isolation model shipped; the paired enrollment model ships in `models/enrollment/`.
- `GetStatus`: `isolation_model` (`pdfnet3-pro`, `pdfnet3-dev`, `none`) and `isolation_model_error` (fixed `MODEL_*` code, never a path); the profile tab shows the code when the model cannot be loaded.
- **Voice enrollment and profile**: guided sample recording, profile build with microphone EQ, owner-only profile storage (`0600`/`0700`), voice isolation on/off and intensity, live profile reload in the virtual microphone.
  - The noisy-environment gate estimates the SNR on the raw recording (speech level against the noise floor of the pauses) and logs `snr_db`; `ENROLL_NOISY_ENVIRONMENT` has its own message.
  - The level gate measures only the active speech, with the same rule and threshold for sample ingest and profile build.
  - Stored samples keep weak speech: a bounded part of the denoiser residual is mixed back (attenuation capped at about 12 dB).
  - The UI follows the profile build job (loading state, resumes a running job; `GetStatus` exposes `active_enrollment_jobs`).
  - Recording feedback: smoothed dBFS level meter, low-speech-level warning and the label of the microphone in use, falling back to the system default microphone.
- **Native virtual microphone**: PipeWire bridge (`libclearcore_filter.so` + `pipewire_helper`) on Linux, WaveRT driver on Windows, CoreAudio HAL on macOS.
- **Studio DSP chain**: `Natural`, `Podcast`, `Broadcast`, `Off` (high-pass, gate/expander, EQ, de-esser, leveler, true-peak limiter).
- **Desktop app** (Electron/React): tray controls, level meters, A/B recording test, status telemetry.

### Behavior
- Every backend request runs the pDFNet3 Pro on tract (CPU); an accelerator request (OpenVINO, TensorRT) reports a CPU fallback with the reason, because those backends cannot feed the FiLM inputs.
- No fallback model: if the bundled pDFNet3 Pro is missing or fails verification, the daemon loads no isolation model and reports `isolation_model: "none"` with the code, and the virtual-microphone filter is not created.
- `voice_profile_supported` is true exactly when the live model can apply a profile; when false, building a profile is refused immediately with `ENROLL_BACKEND_UNSUPPORTED`.
- Enrollment samples are denoised with the same pDFNet3 Pro in neutral conditioning.
- The base DeepFilterNet3 (`models/stateful/`, its TensorRT engines, `vendor/approved/`) is not shipped; it stays in the repository for tests.
- Requires an x86-64 CPU with AVX2 for the isolation model; on ARM64 builds it reports `MODEL_LOAD_FAILED` (known limitation of this beta).
- Licensing: PolyForm Noncommercial License 1.0.0 (source code) and CC BY-NC 4.0 (models and documentation).

[0.1.0-beta.1]: https://github.com/joaorura/clearcore/releases/tag/v0.1.0-beta.1
