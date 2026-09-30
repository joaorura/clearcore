# Production Release Gates (M1 - M6 & GA)

## Overview

Project Hippocamp enforces multi-stage release gates. A release artifact is only eligible for General Availability (GA) when all platform gates pass simultaneously.

---

## Gate Definitions

| Milestone / Gate | Description | Enforcement Mechanism |
|---|---|---|
| **M1** | Filter Model & Golden Reference Validation | Frozen Golden Reference JSON + Corpus Manifest V2. Derive 30ms latency. Dev waiver for > i5-10210U hosts. |
| **M2** | Core Engine, Queues, Silence Contract | 100% offline Docker verification (`--network none`, `--locked`, `--offline`). Zero raw audio leakage. |
| **M3** | Platform Spikes & Virtual Endpoints | Windows WaveRT, macOS CoreAudio HAL, Linux PipeWire native helper verified. |
| **M4** | Host Adapters & Consumer Rebind | WASAPI capture, CoreAudio hidden/visible streams, PipeWire node recreation and rebind handling. |
| **M5** | Desktop UI, Tray & Privacy Diagnostics | Salted device hashes, zero PCM audio in diagnostics export (`realtime-noise-diagnostics`). |
| **M6** | Artifact Scanner, SBOM & Packaging | `scripts/scan-artifacts.sh` (`BLOCKED_UNLISTED_ASSET`), SPDX 2.3 SBOM, WiX/pkg/deb/rpm packages. |
| **GA Simultaneous** | 4-Platform Hardware Qualification | Windows 11, macOS 13+, Ubuntu 24.04, Fedora 44 pass simultaneously. Zero soak dropouts, p95 <= 80ms, p99 <= 7ms. |

---

## Staging & Credential Gates

1. **`BLOCKED_PENDING_HLK` / `BLOCKED_PHYSICAL_WDK_HOST`**: Windows Driver Kit HLK testing requires physical Windows 11 hardware staging rack.
2. **`BLOCKED_PENDING_DEVELOPER_ID` / `BLOCKED_PHYSICAL_MACOS_HOST`**: macOS package signing, notarization, and HAL testing require physical Apple Silicon hardware with active Apple Developer ID certificate.
3. **`BLOCKED_PENDING_UBUNTU_HOST`**: Ubuntu 24.04 verification requires Ubuntu hardware staging node.
4. **`BLOCKED_OFFLINE_DEPENDENCY`**: Offline release packaging requires pre-populated npm and cargo caches.
