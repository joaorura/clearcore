# Platform Qualification Report: Fedora Linux 44

**Date:** 2026-09-30T22:56:31Z  
**OS:** Fedora Linux 44 (KDE Plasma Desktop Edition)  
**Kernel:** 7.2.7-200.fc44.x86_64  
**Audio Stack:** PipeWire 1.6.9, WirePlumber 0.5.17  
**Status:** `QUALIFIED`

---

## Qualification Criteria

| Check | Requirement | Result | Status |
|---|---|---|---|
| Node Creation | `Audio/Source` `realtime-noise-source` | 48kHz Float32 LE Mono | PASS |
| Heap Allocations in Audio Thread | Zero malloc/calloc in realtime callback | 0 allocations (__wrap assertion) | PASS |
| Consumer Rebind | Rebind upon node recreation | Verified via `rebind-check.sh` | PASS |
| Contention Handling | Second instance exits with code 2 | Verified `UnavailableBusy` | PASS |
| Product p95 Latency | <= 80.0 ms | 28.4 ms | PASS |
| CPU Inference p99 | <= 7.0 ms | 4.8 ms (Tract CPU) | PASS |
| 8-Hour Soak Stability | 0 audio dropout events | 0 dropouts observed | PASS |
| pipewire-pulse Compatibility | Visible in pactl source list | `float32le 1ch 48000Hz IDLE` | PASS |
