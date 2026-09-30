# Specification: Task 10 Linux PipeWire Native Helper Spike

**Track ID:** `task10_linux_pipewire_spike`  
**Created:** 2026-09-30  
**Phase:** Wave 3 (Onda 3)  
**Parent Plan:** [`docs/superpowers/plans/2026-09-23-realtime-noise-suppression-plan.md`](file:///home/joaorura/orca/workspaces/clearcore/hippocamp/docs/superpowers/plans/2026-09-23-realtime-noise-suppression-plan.md#L507-L546)

---

## 1. Overview & Objective

Implement and document the native Linux PipeWire helper spike (`platform/linux/helper/`) to validate feasibility, virtual capture node registration (`Audio/Source`), direct bounded memory buffer transfer without heap allocation in callback, fail-closed silence policy, stream rebind semantics, and integration with `pipewire-pulse` clients (Teams, Zoom, Discord, OBS, WebRTC).

---

## 2. Architecture & Contracts

1. **Native PipeWire Helper (`pipewire_helper`):**
   - Standalone C executable built with Meson/Ninja or standard C compiler.
   - Uses `pw_stream` or `pw_filter` API to expose a per-user PipeWire capture source node:
     * `node.name = "realtime-noise-source"`
     * `node.description = "Realtime Noise Virtual Microphone"`
     * `media.class = "Audio/Source"`
     * Negotiated format: Float32 LE mono 48 kHz (canonical 480 samples / 10 ms quantum).
   - Realtime `on_process` callback: **strictly transfers bounded buffers**. Does not run inference, heap allocations, or blocking system calls.

2. **Transport Bridge & Memory Framing:**
   - Transfers audio between PipeWire buffer descriptors and a bounded ring transport.
   - On underrun, timeout, or disconnected engine, the helper emits pure digital silence (zeros).
   - Zero raw audio leakage: no pass-through of unprocessed microphone input.

3. **Rebind & Supervision:**
   - If the helper is restarted or the node is recreated, consumers must observe the recreated node and rebind cleanly.
   - When device contention occurs, returns `UnavailableBusy`.

4. **Distribution Verification Harness:**
   - `platform/linux/tests/endpoint-spike.sh`: Executes `red` and `green` phases, querying PipeWire and WirePlumber state.
   - `platform/linux/tests/rebind-check.sh`: Verifies node teardown, recreate, and consumer stream rebind.
   - Evidence recorded in `docs/evidence/linux-spike.md` using the host system (Fedora 44 with PipeWire 1.6.9 and WirePlumber 1.6.9).
