# Specification: Task 12 Production macOS CoreAudio & HAL Host Adapter

**Track ID:** `task12_macos_production_adapter`  
**Created:** 2026-09-30  
**Phase:** Wave 4 (Onda 4)  
**Parent Plan:** [`docs/superpowers/plans/2026-09-23-realtime-noise-suppression-plan.md`](../../../docs/superpowers/plans/2026-09-23-realtime-noise-suppression-plan.md#L581-L615)

---

## 1. Overview & Objective

Implement production integration for macOS CoreAudio and HAL:
- Refine `platform/macos/HAL/Sources/{HiddenOutput,VisibleInput,RingBuffer}.swift` to ensure lock-free atomic ring buffer clearing on control generation update.
- Refine `platform/macos/Bridge/Sources/RealtimeNoiseBridge/EngineXpc.swift` to synchronize supervisor lifecycle with HAL state.
- Create Swift test suites in `platform/macos/tests/{hotplug,session,endpoint_formats}.swift` validating atomic ring clearing, session arbitration, and fail-closed silence.
- Produce `docs/evidence/macos-integration.md`.

---

## 2. Architecture & Contracts

1. **Lock-Free Atomic Ring & Generation Invalidation:**
   - On control generation change or engine crash/underrun, `RingBuffer.clear()` atomically purges stale audio.
   - Visible input reads immediately produce digital silence (all zeros).
   - Zero raw audio leakage: hardware microphone input is never routed to visible input without processing.

2. **CoreAudio HAL Integration:**
   - Visible virtual microphone input (`AudioDeviceCreate`), 48kHz Float32 mono.
   - Hidden output endpoint receives processed frames from the engine daemon.
   - CoreAudio realtime I/O callback runs in `AudioServerPlugInDriverInterface` with zero dynamic allocations, zero locks, zero synchronous RPCs.

3. **Bridge & Testing:**
   - Swift test suites covering:
     * `testControlGenerationChangeClearsRingAndVisibleInputOutputsSilence`
     * `testConflictingSessionReceivesUnavailableBusy`
     * `testSupportedEndpointFormatsMatch48kHzMonoFloat32`
   - Integration evidence documented in `docs/evidence/macos-integration.md`.
