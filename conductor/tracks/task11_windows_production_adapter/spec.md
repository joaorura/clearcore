# Specification: Task 11 Production Windows WASAPI & WaveRT Host Adapter

**Track ID:** `task11_windows_production_adapter`  
**Created:** 2026-09-30  
**Phase:** Wave 4 (Onda 4)  
**Parent Plan:** [`docs/superpowers/plans/2026-09-23-realtime-noise-suppression-plan.md`](../../../docs/superpowers/plans/2026-09-23-realtime-noise-suppression-plan.md#L547-L580)

---

## 1. Overview & Objective

Implement `platform/windows/host` (`realtime-noise-windows-host`) providing the production Windows audio adapter:
- `AudioBackend` using WASAPI loopback/capture with hotplug resilience (when selected device is lost, enters `WaitingForDevice` without arbitrarily falling back to another microphone).
- `VirtualMicrophone` using PortCls WaveRT driver direct I/O IOCTL transport.
- Enforcing the digital silence policy for `EngineUnavailable`, `Restarting`, and `TerminalSafeState`.

---

## 2. Architecture & Contracts

1. **WASAPI AudioBackend:**
   - Captures microphone input via WASAPI event-driven capture buffer.
   - Converts to canonical 48 kHz mono Float32 audio frames.
   - Hot-plug resilience: On device invalidation (`AUDCLNT_E_DEVICE_INVALIDATED` or unplug), transitions immediately to `DeviceStatus::WaitingForDevice`. Does not automatically switch to a different physical microphone.

2. **WaveRT VirtualMicrophone:**
   - Consumes processed audio from `RealtimeTransport` bounded output queue.
   - Encodes each 480-sample frame into `WireFrameEnvelopeV1`.
   - Submits envelopes via `IOCTL_REALTIME_NOISE_SUBMIT_ENVELOPE` using direct I/O.
   - Under supervisor states `EngineUnavailable`, `Restarting`, or `TerminalSafeState`, the virtual microphone pushes pure digital silence (all zeros).

3. **Workspace & Policy Compatibility:**
   - Package `realtime-noise-windows-host` registered in root `Cargo.toml`.
   - `#![forbid(unsafe_code)]`.
   - All third-party dependencies use `default-features = false`.
   - Compiles cleanly on both Windows (with native APIs) and cross-platform targets (providing contract-preserving simulated implementations and tests in Docker).
