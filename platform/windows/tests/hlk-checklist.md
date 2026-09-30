# Windows Hardware Lab Kit (HLK) Compliance Checklist: RealtimeNoise WaveRT

**Driver Binary:** `RealtimeNoise.sys`  
**Device Hardware ID:** `Root\RealtimeNoise`  
**Target Platform:** Windows 10 / Windows 11 x64 (Universal Driver)  
**Classification:** Kernel Mode Audio Driver (`Class = MEDIA`, PortCls WaveRT)  
**Spike Track:** `task8_windows_wavert_spike`  

---

## 1. Compliance Test Matrix

| Category | HLK Test Name / Requirement | Objective & Verification Standard | Target Outcome | Current Status |
| :--- | :--- | :--- | :--- | :--- |
| **Audio Functional** | `Audio Logo Test - WaveRT (Capture)` | Validates PortCls WaveRT capture pin enumeration, format support (48 kHz mono Float32/PCM16), and cyclic buffer DMA pacing. | Pass with 0 glitches | `BLOCKED_PENDING_HLK` |
| **Audio Functional** | `Audio Roundtrip Latency Test` | Verifies reporting of hardware FIFO latency (10ms / 480 samples) and jitter bounds. | <= 10.0 ms | `BLOCKED_PENDING_HLK` |
| **Audio Functional** | `Audio Format Support & Conversion` | Validates mono 48 kHz Float32 and PCM16 negotiation, rejecting unsupported sample rates or channel counts. | Graceful rejection (`KSERROR_UNSUPPORTED`) | `BLOCKED_PENDING_HLK` |
| **Audio Functional** | `KS Topology & Pin Category Check` | Asserts virtual capture pin exposes `KSCATEGORY_AUDIO`, `KSCATEGORY_CAPTURE`, and `PINNAME_CAPTURE`. | Verified in Registry & KS tree | `BLOCKED_PENDING_HLK` |
| **Fail-Closed Policy** | `Zero Raw Audio Leakage on Idle` | Verifies that when engine is inactive or stopped, capture endpoint produces pure digital zeros (0.0f). Never leaks mic input. | 0 dBFS delta; 100% digital silence | `BLOCKED_PENDING_HLK` |
| **Fail-Closed Policy** | `Underrun Recovery & Silence` | Injects intentional 50ms engine stall; verifies driver outputs silence during gap and resumes clean audio when envelopes resume. | No glitch pop, zero audio leakage | `BLOCKED_PENDING_HLK` |
| **Session Boundary** | `Fast User Switching (FUS) Isolation` | Validates that when Session A locks and Session B logs in, Session B is blocked from hijacking or listening to Session A stream (`UnavailableBusy`). | `STATUS_ACCESS_DENIED` / `STATUS_DEVICE_BUSY` | `BLOCKED_PENDING_HLK` |
| **Session Boundary** | `Handle Cleanup on Process Crash` | Simulates abrupt `SIGKILL` of engine user service; verifies driver cleans up session lock and frees queue immediately. | Next process acquires session cleanly | `BLOCKED_PENDING_HLK` |
| **Device Fundamentals** | `DF - Sleep and Modern Standby with IO (Basic)` | Validates system transitions to S3/S4 and Modern Standby (Connected Standby) while capture stream is active, then resumes without bugcheck. | Clean resume, 0 bugchecks | `BLOCKED_PENDING_HLK` |
| **Device Fundamentals** | `DF - PNP Rebalance & Surprise Removal` | Simulates PnP surprise removal (`IRP_MN_SURPRISE_REMOVAL`) and device disable/enable cycle via Device Manager. | Driver unloads and re-initializes cleanly | `BLOCKED_PENDING_HLK` |
| **Reliability & Verifier**| `Driver Verifier 24-Hour Stress` | Runs HLK Device Fundamentals Stress with Special Pool, Force IRQL Checking, and Deadlock Detection enabled (`0x1423`). | 0 BugChecks (`0xC4`, `0xD1`, `0x0A`) | `BLOCKED_PENDING_HLK` |
| **Security & Signing** | `CodeQL Static Analysis` | Microsoft Recommended CodeQL query suite for C/C++ Drivers (checking for unvalidated user pointer dereference, integer overflow). | 0 high/critical alerts | Pass (Local CodeQL) |
| **Security & Signing** | `HVCI & Hypervisor Enforced Code Integrity` | Driver binary compiled with `/guard:cf`, Spectre mitigations, and compatible with Memory Integrity (HVCI). | Compatible with Core Isolation | Pass (Compiler Flags) |
| **Security & Signing** | `WHQL Production Driver Signing` | Signed with Extended Validation (EV) Code Signing Certificate and submitted to Microsoft Hardware Developer Portal. | Signed `.cat` catalog file | `BLOCKED_PHYSICAL_WDK_HOST` |

---

## 2. Driver Verifier Stress Specification

To prepare the physical staging rack machine for the 24-hour HLK stress test:

1. Enable Driver Verifier flags:
   ```powershell
   powershell -File platform/windows/tests/driver_verifier.ps1 -Action Enable
   ```
2. Reboot physical test runner.
3. Verify shims are active:
   ```powershell
   powershell -File platform/windows/tests/driver_verifier.ps1 -Action Status
   ```
4. Attach kernel debugger (WinDbg over Net / KDNET) to monitor live IRQL and spinlock acquisitions.
5. Execute HLK Device Fundamentals Audio test schedule.
6. Verify no bugchecks logged:
   ```powershell
   powershell -File platform/windows/tests/driver_verifier.ps1 -Action CheckViolations
   ```

---

## 3. Physical Staging Gate Rationale

Tests marked `BLOCKED_PENDING_HLK` and `BLOCKED_PHYSICAL_WDK_HOST` cannot be executed inside Linux build containers or headless virtualized runners. They require:
- A physical Windows 11 x64 test node connected to a Windows HLK Controller over Gigabit Ethernet.
- Physical audio loopback or WASAPI loopback test fixtures.
- An EV Code Signing Hardware Security Module (HSM) for WHQL release attestation.
