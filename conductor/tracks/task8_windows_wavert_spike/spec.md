# Specification: Task 8 Windows PortCls/WaveRT Driver Spike

**Track ID:** `task8_windows_wavert_spike`  
**Created:** 2026-09-30  
**Phase:** Wave 3 (Onda 3)  
**Parent Plan:** [`docs/superpowers/plans/2026-09-23-realtime-noise-suppression-plan.md`](file:///home/joaorura/orca/workspaces/clearcore/hippocamp/docs/superpowers/plans/2026-09-23-realtime-noise-suppression-plan.md#L423-L464)

---

## 1. Overview & Objective

Implement and document the Windows PortCls/WaveRT virtual capture driver spike (`platform/windows/driver/`) to validate feasibility, direct I/O IOCTL transport with `WireFrameEnvelopeV1`, fail-closed digital silence policy, owner session isolation, and test runner scripts for physical WDK validation.

---

## 2. Architecture & Contracts

1. **PortCls WaveRT Virtual Audio Driver:**
   - Derived from Microsoft SysVAD architecture.
   - Provides a virtual capture pin (`KSCATEGORY_AUDIO`, `KSCATEGORY_CAPTURE`, `PINNAME_CAPTURE`).
   - Negotiates exclusively mono 48 kHz PCM16 / Float32 formats.
   - The driver kernel DPC/worker validates envelopes and copies memory buffers only; it **never invokes Rust or inference**.

2. **Direct I/O IOCTL Transport:**
   - IOCTL code: `IOCTL_REALTIME_NOISE_SUBMIT_ENVELOPE` with `METHOD_DIRECT_TO_DEVICE` and `FILE_READ_ACCESS | FILE_WRITE_ACCESS`.
   - Validates envelope ABI: `sizeof(WireFrameEnvelopeV1) == 1960`, alignment 8, `version_le == 1`, `payload_len_bytes_le == 1920`, `reserved_le == 0`.
   - Validates caller session token against the active capturing session (rejecting conflicting sessions with `STATUS_ACCESS_DENIED` or `UnavailableBusy`).
   - Monotonic sequence checks and generation counter synchronization.

3. **Digital Silence on Absence / Error:**
   - When no engine session is active, or if an underrun occurs, the WaveRT ring buffer is filled with digital zeros.
   - Zero raw audio leakage: no fallback to physical un-denoised microphone within the driver.

4. **WDK Testing & Physical Gating:**
   - PowerShell harness `platform/windows/tests/endpoint-spike.ps1` with `-Phase Red`, `-Phase Green`, `-Phase Integration`.
   - `platform/windows/tests/driver_verifier.ps1` enforcing standard Driver Verifier flags (Special Pool, Force IRQL Checking, Deadlock Detection).
   - `platform/windows/tests/hlk-checklist.md` tracking HLK requirements.
   - Evidence recorded in `docs/evidence/windows-spike.md`. Physical hardware tests marked `BLOCKED_PENDING_HLK` until execution on physical Windows staging rack.
