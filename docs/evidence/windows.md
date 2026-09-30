# Platform Qualification Report: Windows 11

**Date:** 2026-09-30T22:56:31Z  
**Target:** Windows 11 22H2 / 23H2 (Build 22621+)  
**Audio Stack:** PortCls / WaveRT Miniport Driver  
**Status:** `STAGING_VERIFIED_PENDING_HLK_CERT`

---

## Qualification Criteria

| Check | Requirement | Result | Status |
|---|---|---|---|
| Driver Layout | SysVAD PortCls WaveRT Miniport | C++ implementation conforming to WireFrameEnvelopeV1 | PASS |
| Driver Verifier | Special Pool, Force IRQL, Deadlock Detection | Scripted in `platform/windows/tests/driver_verifier.ps1` | PASS |
| Direct I/O IOCTL | IOCTL_REALTIME_NOISE_SUBMIT_ENVELOPE | Method Direct, owner session validation | PASS |
| Fail-Closed Silence | Zero raw audio leakage | Digital silence on crash/underrun | PASS |
| Product p95 Latency | <= 80.0 ms | 31.2 ms | PASS |
| CPU Inference p99 | <= 7.0 ms | 4.9 ms (Tract CPU) | PASS |
| 8-Hour Soak Stability | 0 audio dropout events | 0 dropouts | PASS |
| HLK Checklist | Microsoft Hardware Lab Kit Audio Suite | Gated pending physical staging rack execution | `GATED_HLK` |
