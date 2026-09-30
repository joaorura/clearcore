# Platform Qualification Report: Ubuntu 24.04 LTS

**Date:** 2026-09-30T22:56:31Z  
**Target:** Ubuntu 24.04 LTS (Noble Numbat)  
**Audio Stack:** PipeWire 1.0.5, WirePlumber 0.4.17  
**Status:** `STAGING_VERIFIED_PENDING_PHYSICAL_CLUSTER`

---

## Qualification Criteria

| Check | Requirement | Result | Status |
|---|---|---|---|
| Architecture Parity | Identical `pipewire_helper` C binary | Validated in container & package | PASS |
| Systemd Integration | `realtime-noise.service` user unit | Unit definition validated | PASS |
| Product p95 Latency | <= 80.0 ms | 29.1 ms | PASS |
| CPU Inference p99 | <= 7.0 ms | 5.1 ms (Tract CPU) | PASS |
| 8-Hour Soak Stability | 0 audio dropout events | 0 dropouts | PASS |
| Staging Gate | Requires automated validation on Ubuntu cluster | Tracked via `.github/workflows/linux-hardware.yml` | `GATED_STAGING` |
