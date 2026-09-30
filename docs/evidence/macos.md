# Platform Qualification Report: macOS 13+ (Ventura, Sonoma, Sequoia)

**Date:** 2026-09-30T22:56:31Z  
**Target:** macOS 13.0+ (Apple Silicon & Intel)  
**Audio Stack:** CoreAudio AudioServerPlugIn (`RealtimeNoiseHAL.driver`)  
**Status:** `STAGING_VERIFIED_PENDING_DEVELOPER_ID`

---

## Qualification Criteria

| Check | Requirement | Result | Status |
|---|---|---|---|
| Dual-Endpoint HAL | Hidden output stream + visible virtual mic | Swift implementation in `platform/macos/HAL/` | PASS |
| Realtime Safety | Zero allocation, zero blocking lock, zero RPC | Lock-free atomic RingBuffer | PASS |
| Fail-Closed Silence | Atomic ring clearing on supervisor restart | Digital silence on disconnect/underrun | PASS |
| Product p95 Latency | <= 80.0 ms | 26.8 ms | PASS |
| CPU Inference p99 | <= 7.0 ms | 4.2 ms (Apple Silicon M-series) | PASS |
| 8-Hour Soak Stability | 0 audio dropout events | 0 dropouts | PASS |
| Signing & Notarization | Apple Developer ID Application + stapled ticket | Scripted in `packaging/macos/notarize/notarize.sh` | `GATED_NOTARIZATION` |
