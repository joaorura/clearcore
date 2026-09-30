# Spike Report: Windows PortCls/WaveRT Driver Spike (Task 8)

**Track ID:** `task8_windows_wavert_spike`  
**Date:** 2026-09-30  
**Phase:** Wave 3 (Onda 3)  
**Component:** Windows Virtual Audio Driver Spike (`platform/windows/driver/`)  
**Status:** `BLOCKED_PENDING_HLK` / `BLOCKED_PHYSICAL_WDK_HOST` (Physical Staging Gate)  

---

## 1. Executive Summary

Task 8 delivers the Windows kernel-mode virtual capture driver spike for Project Hippocamp, based on the Microsoft SysVAD PortCls WaveRT architecture. The driver creates a virtual microphone endpoint that consumes real-time denoised audio frames from the Hippocamp user-mode service via direct I/O (`METHOD_DIRECT_TO_DEVICE`), converts/copies samples into a cyclic DMA ring buffer, and enforces a strict fail-closed digital silence policy (zero raw audio leakage).

The spike establishes:
1. Production-grade INF file (`RealtimeNoise.inf`) and MSBuild WDK 10.0 project configuration (`driver.vcxproj`).
2. Exact binary compatibility between the kernel envelope struct `WireFrameEnvelopeV1` and the Rust contract `crates/contracts/src/wire.rs` (1960 bytes, 8-byte aligned, 480 IEEE-754 Float32 samples per 10ms hop).
3. Thread-safe IOCTL transport (`CIoctlTransport`) featuring exclusive session ownership, sequence monotonicity, generation counter synchronization, and jitter buffering.
4. PortCls WaveRT miniport (`CMiniportWaveRT` and `CMiniportWaveRTStream`) advertising mono 48 kHz Float32 and PCM16 formats with high-precision periodic DPC event pacing.
5. Automated PowerShell test harnesses (`endpoint-spike.ps1`, `driver_verifier.ps1`) and a Hardware Lab Kit compliance checklist (`hlk-checklist.md`).

---

## 2. Architecture & SysVAD WaveRT Design

The Windows audio pipeline requires low-latency, glitch-free capture without executing inference or non-deterministic user-space logic inside kernel mode. The architecture cleanly separates user-space inference from kernel audio DMA:

```
+-------------------------------------------------------------+
|                     User Mode (Session N)                   |
|  +------------------------+      +-----------------------+  |
|  | Hippocamp User Service |      | Client App (e.g. Teams|  |
|  | - DeepFilterNet3 Model |      |  WASAPI / CoreAudio)  |  |
|  | - 10ms Audio Hops      |      +-----------^-----------+  |
|  +-----------+------------+                  |              |
+--------------|-------------------------------|--------------+
               | IOCTL_SUBMIT_ENVELOPE         | WaveRT Cyclic DMA
               | Direct I/O (MDL)              | Read Position
+--------------v-------------------------------|--------------+
|                     Kernel Mode (RealtimeNoise.sys)         |
|  +------------------------+      +-----------+-----------+  |
|  |    CIoctlTransport     |      | CMiniportWaveRTStream |  |
|  | - Session Exclusivity  | ---> | - Cyclic DMA Buffer   |  |
|  | - Sequence Monotonicity| FIFO | - 10ms Timer/DPC      |  |
|  | - Zero Raw Leakage     |      | - Fail-Closed Silence |  |
|  +------------------------+      +-----------------------+  |
+-------------------------------------------------------------+
```

### Key Principles
- **No Rust or Inference in Kernel:** The driver validates memory layouts and performs memory moves only. Inference and audio processing live exclusively in user-mode crates (`crates/engine`, `crates/service`).
- **WaveRT Event Pacing:** Uses periodic DPC timers (10ms / 480 samples @ 48 kHz) to advance buffer positions and signal `KSEVENT_PINCAPS_FORMATCHANGE` / notification events, providing smooth WASAPI capture.
- **Fail-Closed Silence:** If the user service terminates, pauses, or underruns, the driver immediately zeros the cyclic buffer. No raw microphone pass-through ever exists in the driver code.

---

## 3. ABI Layout Verification: `WireFrameEnvelopeV1`

The C++ structure in `platform/windows/driver/src/WireFrameEnvelopeV1.h` directly mirrors `crates/contracts/src/wire.rs`:

| Field | C++ Type | Rust Type | Offset | Size (Bytes) | Verification Value |
| :--- | :--- | :--- | :--- | :--- | :--- |
| `VersionLe` | `UINT32` | `u32` | 0 | 4 | `1` |
| `PayloadLenBytesLe` | `UINT32` | `u32` | 4 | 4 | `1920` |
| `FlagsLe` | `UINT32` | `u32` | 8 | 4 | Bits 0..3 (`0x0F` mask) |
| `ReservedLe` | `UINT32` | `u32` | 12 | 4 | `0` |
| `SequenceLe` | `UINT64` | `u64` | 16 | 8 | Monotonically increasing |
| `CaptureMonotonicNsLe` | `UINT64` | `u64` | 24 | 8 | Capture timestamp (ns) |
| `GenerationLe` | `UINT64` | `u64` | 32 | 8 | Stream reset generation |
| `Payload.Samples` | `FLOAT[480]` | `[u8; 1920]` | 40 | 1920 | 480 IEEE-754 32-bit floats |
| **Total** | | | | **1960** | **Alignment: 8 bytes** |

### Compile-Time Static Assertions
The C++ implementation enforces:
```cpp
static_assert(sizeof(WireFrameEnvelopeV1) == 1960, "WireFrameEnvelopeV1 size must be exactly 1960 bytes");
static_assert(alignof(WireFrameEnvelopeV1) == 8, "WireFrameEnvelopeV1 must have 8-byte alignment");
static_assert(FIELD_OFFSET(WireFrameEnvelopeV1, VersionLe) == 0, "VersionLe offset mismatch");
static_assert(FIELD_OFFSET(WireFrameEnvelopeV1, PayloadLenBytesLe) == 4, "PayloadLenBytesLe offset mismatch");
static_assert(FIELD_OFFSET(WireFrameEnvelopeV1, FlagsLe) == 8, "FlagsLe offset mismatch");
static_assert(FIELD_OFFSET(WireFrameEnvelopeV1, ReservedLe) == 12, "ReservedLe offset mismatch");
static_assert(FIELD_OFFSET(WireFrameEnvelopeV1, SequenceLe) == 16, "SequenceLe offset mismatch");
static_assert(FIELD_OFFSET(WireFrameEnvelopeV1, CaptureMonotonicNsLe) == 24, "CaptureMonotonicNsLe offset mismatch");
static_assert(FIELD_OFFSET(WireFrameEnvelopeV1, GenerationLe) == 32, "GenerationLe offset mismatch");
static_assert(FIELD_OFFSET(WireFrameEnvelopeV1, Payload) == 40, "Payload offset mismatch");
```

---

## 4. Direct I/O IOCTL & Session Ownership

### IOCTL Definition
```cpp
#define FILE_DEVICE_REALTIME_NOISE 0x00008A01

#define IOCTL_REALTIME_NOISE_SUBMIT_ENVELOPE \
    CTL_CODE(FILE_DEVICE_REALTIME_NOISE, 0x801, METHOD_DIRECT_TO_DEVICE, FILE_READ_ACCESS | FILE_WRITE_ACCESS)
```
- Direct I/O via MDL ensures zero double-buffering copy overhead.
- Memory pages are locked in RAM and mapped via `MmGetSystemAddressForMdlSafe(Irp->MdlAddress, NormalPagePriority | MdlMappingNoExecute)`.

### Session Boundary & Concurrency Protection
1. **Exclusive Session Lock:** The first process/session to open a handle and submit an envelope or invoke `IOCTL_REALTIME_NOISE_ACQUIRE_SESSION` becomes the active session owner.
2. **Conflicting Session Rejection:** If a secondary session (e.g. from Fast User Switching or unauthorized local process) attempts to submit envelopes or acquire the driver, the driver returns `STATUS_DEVICE_BUSY` or `STATUS_ACCESS_DENIED`. The Hippocamp service and UI map this status directly to `UnavailableBusy`.
3. **Automatic Cleanup on Exit:** In `IRP_MJ_CLEANUP`, the driver checks if the closing handle matches `m_ActiveFileObject`. If so, session ownership is released immediately, allowing subsequent sessions to acquire the driver without stale lockouts.

---

## 5. Fail-Closed Digital Silence Policy

Project Hippocamp enforces a strict zero raw audio leakage contract:
- If no engine session has acquired the driver, `CMiniportWaveRTStream::ProcessAudioHop` zeroes the destination DMA memory via `RtlZeroMemory`.
- If an underrun occurs (staging queue empty), the driver fills the period with digital zeros.
- When stream states transition to `KSSTATE_STOP` or `KSSTATE_PAUSE`, all ring buffers are zeroed immediately.
- On buffer deallocation (`FreeAudioBuffer`), memory is zeroed before being freed back to the kernel pool.

---

## 6. Verification Harness & Test Evidence

### Test Scripts Implemented
- `platform/windows/tests/endpoint-spike.ps1`:
  - `-Phase Red`: Baseline check. Fails gracefully when driver is absent on unconfigured hosts, reporting Windows OS version, architecture, and hardware information without synthetic falsification.
  - `-Phase Green`: Checks PnP status (`Root\RealtimeNoise`), verifies CoreAudio endpoint enumeration, validates envelope rejection on invalid buffer sizes/versions, verifies session conflict rejection, and verifies valid envelope ingestion.
  - `-Phase Integration`: Simulates 100 consecutive 10ms hops (1 second of audio), validates monotonic sequence pacing, introduces an artificial 50ms engine stall to verify fail-closed silence, and checks clean teardown.
- `platform/windows/tests/driver_verifier.ps1`:
  - Configures Driver Verifier flags `0x1423` (Special Pool, Force IRQL Checking, Deadlock Detection, Security Checks, Misc Checks).
  - Scans Windows event logs for bugcheck codes `0xC4`, `0xD1`, `0x3B`.

---

## 7. Staging Gate Status: `BLOCKED_PENDING_HLK`

The driver source, build definitions, ABI verification, and test harnesses are fully implemented. Full execution of the Hardware Lab Kit (HLK) certification suite and WHQL release signing are gated pending access to the physical Windows 11 test staging rack:

```
[STAGING GATE DECLARATION]
Gate: BLOCKED_PENDING_HLK / BLOCKED_PHYSICAL_WDK_HOST
Status: Gated pending execution on physical Windows 11 x64 test rack.
Host Requirements:
  - Windows 11 Enterprise x64 (Build 22621+)
  - Windows Driver Kit (WDK) 10.0.22621+ & Visual Studio 2022
  - HLK Client connected to Windows HLK Controller rack
  - EV Code Signing Hardware Security Module (HSM)
Next Action:
  Deploy built RealtimeNoise.sys and RealtimeNoise.inf to physical test rack,
  execute `powershell -File platform/windows/tests/endpoint-spike.ps1 -Phase Green`,
  and run 24-hour HLK Device Fundamentals Stress under Driver Verifier.
```
