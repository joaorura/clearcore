# Specification: Task 9 macOS HAL Audio Server Plug-in Spike

**Track ID:** `task9_macos_hal_spike`  
**Created:** 2026-09-30  
**Phase:** Wave 3 (Onda 3)  
**Parent Plan:** [`docs/superpowers/plans/2026-09-23-realtime-noise-suppression-plan.md`](../../../docs/superpowers/plans/2026-09-23-realtime-noise-suppression-plan.md#L465-L506)

---

## 1. Overview & Objective

Implement and document the macOS CoreAudio HAL Audio Server Plug-in (`.driver` bundle) spike (`platform/macos/HAL/`) and bridge library (`platform/macos/Bridge/`) to validate feasibility, dual-endpoint topology (hidden output + visible input), atomic lock-free ring buffer, fail-closed silence policy, out-of-callback XPC communication, and test runner scripts for Apple Silicon verification.

---

## 2. Architecture & Contracts

1. **Audio Server Plug-in (HAL):**
   - CoreAudio HAL plugin complying with `AudioServerPlugInDriverInterface`.
   - Dual-endpoint design:
     * **Visible Input:** Exposed to system as a virtual microphone input (`AudioDeviceCreate`), 48 kHz Float32 mono, selectable by Teams, Zoom, Discord, OBS, WebRTC.
     * **Hidden Output:** Internal loopback endpoint consumed by the `realtime-noise-service` engine writer.
   - CoreAudio realtime I/O callback reads directly from the lock-free ring buffer. **HAL callbacks never perform heap allocations, synchronous RPCs, or file I/O**.

2. **Atomic Lock-Free RingBuffer:**
   - Synchronized ring buffer with sequence tracking and atomic generation invalidation.
   - On control generation update, or if engine crashes/underruns, the ring atomically clears and the visible input outputs digital silence (all zeros).
   - Zero raw audio leakage: un-denoised audio from the hardware microphone is never bridged into the visible input.

3. **Bridge & XPC Communication:**
   - Out-of-callback XPC client (`EngineXpc.swift`) communicating with `realtime-noise-service` to negotiate session ownership, generation count, and device state.
   - Conflicting non-owner sessions receive `UnavailableBusy`.

4. **Physical Apple Silicon Staging Gate:**
   - Shell harness `platform/macos/tests/endpoint-spike.sh` with `red`, `green`, `integration` modes.
   - Evidence recorded in `docs/evidence/macos-spike.md`. Physical notarization and packaging gated under `BLOCKED_PENDING_DEVELOPER_ID` until execution on physical macOS hardware.
