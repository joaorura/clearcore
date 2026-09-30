# Specification: Task 13 Production Linux PipeWire Host Adapter

**Track ID:** `task13_linux_production_adapter`  
**Created:** 2026-09-30  
**Phase:** Wave 4 (Onda 4)  
**Parent Plan:** [`docs/superpowers/plans/2026-09-23-realtime-noise-suppression-plan.md`](file:///home/joaorura/orca/workspaces/clearcore/hippocamp/docs/superpowers/plans/2026-09-23-realtime-noise-suppression-plan.md#L616-L650)

---

## 1. Overview & Objective

Implement `platform/linux/host` (`realtime-noise-linux-host`) providing the production Linux PipeWire adapter:
- `AudioBackend` capturing input from PipeWire audio sources with device hotplug handling.
- `VirtualMicrophone` supervising the per-user `pipewire_helper` process, maintaining node recreation and consumer rebind semantics.
- Handling device contention (`UnavailableBusy`) and enforcing fail-closed digital silence on supervisor state transitions (`EngineUnavailable`, `Restarting`, `TerminalSafeState`).

---

## 2. Architecture & Contracts

1. **PipeWire AudioBackend:**
   - Captures microphone input via PipeWire stream interface or child process bridge.
   - Hot-plug resilience: On input source disconnect, transitions to `DeviceStatus::WaitingForDevice` without arbitrarily falling back to another microphone.

2. **PipeWire VirtualMicrophone:**
   - Manages and supervises the per-user `pipewire_helper` native process.
   - Pushes processed frames from `RealtimeTransport` bounded output queue into helper transport.
   - Rebind handling: When helper recreation occurs, consumers observe node recreation and rebind before audio is delivered.
   - Fail-closed digital silence policy on engine failure or restart.

3. **Workspace & Policy Compatibility:**
   - Package `realtime-noise-linux-host` registered in root `Cargo.toml`.
   - `#![forbid(unsafe_code)]`.
   - All third-party dependencies use `default-features = false`.
   - 100% offline Docker test verification under `--features tract --locked --offline`.
