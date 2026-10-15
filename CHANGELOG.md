# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

ClearCore is a first-party, cross-platform realtime AI noise-suppression virtual microphone powered by DeepFilterNet3 ONNX, ONNX Runtime (`ort` v2) neural streaming, Target Speaker Extraction (DFNet-TSE-MHCA), PipeWire C bridge (Linux), WaveRT PortCls kernel driver (Windows), CoreAudio HAL (macOS), and a resilient Rust supervisor daemon.

## [0.1.0-beta.2] - 2026-10-15

### Fixed
- Voice profile enrollment failed on every recording: the noisy-environment gate estimated the noise as the raw-minus-denoised difference, which counted the voice itself removed by the denoiser as noise. The gate now estimates the SNR on the raw signal (speech level against the noise floor of the pauses, with digital silence treated as a quiet environment) and logs `snr_db`.
- The `ENROLL_NOISY_ENVIRONMENT` code was not recognized by the Electron layer and was shown as "O serviço não conseguiu processar o áudio". It now shows the correct noisy-environment message.

## [0.1.0-beta.1] - 2026-10-13

### Added
- **Realtime AI Noise Suppression & Target Speaker Extraction (TSE)**:
  - Neural engine powered by **ONNX Runtime (`ort` v2)** with explicit stateful graph execution (`enc.onnx`, `erb_dec.onnx`, `df_dec.onnx`) pinned at $S = 1$ (10 ms hop = 480 samples @ 48 kHz).
  - Advanced **DFNet-TSE-MHCA** model: incorporates $K = 16$ multi-token speaker memory dictionary, sub-band cross-attention (MHCA, 4 heads, $D = 128$) with precomputed offline KV-caching (`precompute_kv`), and Staged Condition Injection (SCI) to completely eliminate same-sex speaker confusion.
  - Trained and verified with unified multi-task loss including Additive Angular Margin Loss (**ArcFace**, $s = 32.0, m = 0.25$ rad), asymmetric formant preservation, and temporal DF regularizations.
- **Zero-Allocation Audio Thread**:
  - Pre-allocated I/O and state tensors with IO Binding guarantee **100% zero heap allocations (`0 bytes malloc`)** in the realtime audio callback.
  - Fail-closed digital silence policy: outputs clean silence upon buffer underflow or invalid samples without leaking private acoustic data.
- **Native OS Virtual Device Drivers**:
  - **Linux (PipeWire)**: Native C bridge (`libclearcore_filter.so` + `pipewire_helper`) creating per-user `realtime-noise-source` node with dynamic profile hot-reloading (250 ms timer and `SIGUSR1`).
  - **Windows (WaveRT / WASAPI)**: WaveRT PortCls SysVAD kernel-streaming driver (`RealtimeNoise.inf` / `RealtimeNoise.sys`) with localhost IPC bridge.
  - **macOS (CoreAudio)**: AudioServerPlugIn HAL driver (`RealtimeNoiseHAL.driver`) and lock-free Swift ring buffers.
- **Studio DSP Chain**:
  - Integrated deterministic studio-grade finishing chain (`Natural`, `Podcast`, `Broadcast`, `Off`): high-pass filter, noise gate/expander, parametric EQ, de-esser, fast leveler, and transparent true-peak limiter.
- **Voice Enrollment & Biometric Profiling**:
  - 5-sample guided calibration workflow: records and denoises raw PCM in memory, calculates microphone acoustic EQ curves, and embeds speaker identity.
  - Secure biometric storage: profiles encrypted and stored with owner-only permissions (`0600`/`0700`) under `~/.local/share/clearcore/profiles/`.
- **Desktop Companion Application**:
  - Self-contained desktop app (Electron / React) with system tray controls, real-time input/output VU metering, A/B audio recording comparison test card, and status telemetry.

### Changed
- Complete architecture migration to ONNX Runtime (`ort` v2), providing hardware-accelerated AVX2 / AVX-512 VNNI execution and sub-millisecond per-frame latency (< 0.28 ms).
- Licensing: Released under PolyForm Noncommercial License 1.0.0 (source code) and CC BY-NC 4.0 (models and documentation).

[0.1.0-beta.2]: https://github.com/joaorura/clearcore/compare/v0.1.0-beta.1...v0.1.0-beta.2
[0.1.0-beta.1]: https://github.com/joaorura/clearcore/releases/tag/v0.1.0-beta.1
