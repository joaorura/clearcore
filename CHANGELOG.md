# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

ClearCore is a first-party, cross-platform realtime AI noise-suppression virtual microphone powered by DeepFilterNet3 ONNX, Tract AVX2 acceleration, PipeWire C bridge (Linux), WaveRT PortCls kernel driver (Windows), CoreAudio HAL (macOS), and a resilient Rust supervisor daemon.

## [Unreleased]

### Added
- Voice profile activation (development-integrated): `InferenceBackend` gains `set_voice_profile`, backends that cannot condition on a profile reject it explicitly, and the service applies `SetVoiceProfile` / `ClearVoiceProfile` transactionally (verify, apply to the backend, persist; rollback on failure) and re-applies the stored profile on every backend selection. The packaged virtual microphone (`filter-capi` / helper) does not apply the profile yet.

- Voice enrollment pipeline (development-integrated): samples are denoised in memory with the base DFNet3 and stored as 16-bit 48 kHz mono WAV (the service never writes the raw recording to disk, and core dumps are disabled in the supported launches: systemd unit, Electron app and scripts), each microphone has a budget of 90 s of speech, and the profile is built from the samples of the most recent microphone by concatenating the trimmed speech (silence removed, levels matched, microphone EQ estimated) and enrolled with a development model that is loaded only through `CLEARCORE_DEV_ENROLLMENT_ASSET` and `CLEARCORE_DEV_ENROLLMENT_SHA256`. Scope: the packaged virtual microphone does not apply the profile yet, and no improvement of voice isolation is claimed.
- IPC: `AddVoiceSample` (now carries PCM and the capture device), `BuildVoiceProfile` and `GetEnrollmentJob`, with fixed `ENROLL_*` error codes (including `ENROLL_BUSY`). Jobs answer immediately, at most two run at once per job table (sample/take jobs and profile jobs), a job still running after 10 minutes is failed by a watchdog (`ENROLL_FAILED`, stage `timeout`), and their result is applied on the next request, so clients poll `GetEnrollmentJob`. `ListVoiceSamples` adds `speech_seconds`, `device_label`, `used_in_profile`, `needs_reenroll`, `other_microphone` and `budget`. Call takes over the budget margin are skipped without an error and approving one above the budget is refused while the take is kept. See `docs/ipc-v1.md`.
- Enrollment ingestion by jobs: `AddVoiceSample` and `AddIntakeSuggestion` answer with a `sample-job-N` id and `BuildVoiceProfile` with a `profile-job-N` id; `GetEnrollmentJob` looks in both tables and also reports `take_id`, `recorded` and `reason`. Requests beyond two running jobs in their table, a build without a microphone group or with under 6 s of speech, and a sample or take sent to a service without a base denoiser answer immediately (`ENROLL_BUSY`, `ENROLL_TOO_LITTLE_SPEECH`, `ENROLL_MODEL_NOT_CONFIGURED`) without creating a job, and the build reads only `samples/<id>.wav`.
- Speech budget per microphone group: `ListVoiceSamples` adds `device_id_hash` per item plus `selected_device_id_hash` and `selected_device_label` (null without eligible samples) and no longer exposes `audio_path`; call takes are stored only when they fit the budget (a take above it is skipped with `recorded: false, reason: "budget"`), and approving a take that no longer fits is refused while the take is kept. The old `AddIntakeSuggestion` form without PCM is refused with `INVALID_COMMAND`.
- Samples from before the pipeline are migrated once, all at startup, as `needs_reenroll` entries.
- IPC connections have a 30 s read and write timeout, the service serves one client at a time, and a request line over 32 MiB is answered with `ENROLL_PAYLOAD_TOO_LARGE` (the connection stays usable).
- App: the voice profile card is organized in accessible tabs (generic tabs component), the development model notice stays above the tabs and the stepper is locked while recording.
- App: the capture records raw PCM from the physical microphone, with no fallback to an unconstrained stream, and the voice card shows the speech budget meter, per-sample quality and the delete prompt when the budget is exceeded.

### Fixed
- `BuildVoiceProfile` no longer ends `failed`/`apply` with the generic `ENROLL_FAILED` when the active backend cannot apply a voice profile (OpenVINO/NPU always, tract with the approved base DFNet3, which has no `FiLM` inputs): a new closed-list code `ENROLL_BACKEND_UNSUPPORTED` is answered immediately, without creating a job, when the backend reports no support (`InferenceBackend::supports_voice_profile`), and by the `apply` stage when the backend refuses the built profile; persistence failures stay `ENROLL_FAILED`. `GetStatus` adds `voice_profile_supported` and `neural_eq_calibrated` (EQ of the applied profile), and the app shows the unsupported notice and disables "Generate profile" in the Profile tab. See `docs/ipc-v1.md`.
- IPC hardening: request lines are capped at 32 MiB (`ENROLL_PAYLOAD_TOO_LARGE`) with a bounded read buffer, malformed requests (including a line that is not valid UTF-8) get `JSON_PARSE_ERROR` with the fixed message `malformed request`, `VERSION_MISMATCH` no longer echoes the client version, echoed request ids are cut to 64 characters, and a transport error ends the session.
- Diagnostics no longer include voice samples, WAV files or profiles.
- `GetStatus` now reports the truth about voice profiles: `active_voice_profile_id` / `is_voice_profile_active` mean "applied on the service backend", while `stored_voice_profile_id` / `voice_profile_selected` mean "persisted on disk", with a generic `voice_profile_error` when they differ. The error field never carries backend error text or profile data.
- The app-tauri TypeScript build is repaired after the voice profile changes.

### Changed
- `GetVoiceProfileEmbedding` is deprecated and always answers the fixed error `DEPRECATED` (status `InvalidCommand`); the voice profile no longer stores a placeholder embedding. The voice samples move to the profile store directory (one-time migration; samples without audio are flagged `needs_reenroll`).
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
