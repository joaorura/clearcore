# Specification: Task 5 Realtime Noise Engine, Queues, Generations & Silence Policy

**Track ID:** `task5_engine_queues_generations`  
**Created:** 2026-09-30  
**Phase:** Wave 2 (Onda 2)  
**Parent Plan:** [`docs/superpowers/plans/2026-09-23-realtime-noise-suppression-plan.md`](../../../docs/superpowers/plans/2026-09-23-realtime-noise-suppression-plan.md#L287-L331)

---

## 1. Overview & Objective

Implement `crates/engine` (`realtime-noise-engine`), providing the real-time audio processing coordinator (`DenoiseEngine`), lock-free bounded inter-thread queues, generation lifecycle tracking, and strict fail-closed silence policies.

The engine coordinates the ingest from audio input callbacks, dispatches inference to an off-thread worker implementing `InferenceBackend` (e.g. `TractBackend`), and delivers processed frames to audio output endpoints implementing `RealtimeTransport`.

---

## 2. Core Functional Requirements

1. **`DenoiseEngine` Public API:**
   - `start(&mut self) -> Result<(), EngineError>`
   - `stop(&mut self) -> Result<(), EngineError>`
   - `set_mode(&mut self, mode: DenoiseMode)`:
     - `DenoiseMode::Active`: Processes audio through the selected `InferenceBackend`.
     - `DenoiseMode::Bypass`: Passes frames directly to output without calling inference, maintaining framing and endpoint contracts (no raw bypass fallback).
     - `DenoiseMode::Mute`: Immediately emits digital silence (`[0.0; 480]`).
   - `set_backend(&mut self, backend: Box<dyn InferenceBackend>) -> Result<(), EngineError>`: Switches inference backend strictly on a 480-sample hop boundary.
   - `begin_generation_restart(&mut self, reason: ResetReason) -> Result<u64, EngineError>`: Initiates off-thread generation rebuild and warm-up.
   - `status(&self) -> EngineStatus`

2. **Bounded Queue Architecture:**
   - Queue capacity: strictly 24 hops (240 ms @ 48 kHz).
   - High-watermark age limit: 20 ms or 2 hops, whichever comes first, closes generation and restarts.
   - Valid generation queue age: $\le 10\text{ ms}$.
   - Backpressure policy: when the queue is full, discard the newest frame and record discontinuity; never block the real-time audio thread.

3. **Fail-Closed Silence Policy:**
   - Any worker crash, thread panic, buffer overrun, or inference deadline miss (> 10.0 ms) immediately forces output to complete digital silence until a warmed replacement generation is ready.
   - Zero raw audio leaks to endpoints on failure.

4. **Generation Lifecycle:**
   - Monotonically increasing generation IDs (`u64`).
   - Old/stale frames from superseded generations are discarded cleanly.
   - Warm-up requires one ordered pass of silent frames or initialization frames off the audio thread before transitioning to active output.

---

## 3. Non-Functional & Safety Constraints

- **Strict `#![forbid(unsafe_code)]`**: No raw pointer manipulation or unsafe concurrency.
- **Zero Allocation in Process Loop**: The real-time worker loop must not allocate or reallocate memory during steady-state hop processing.
- **Strict TDD**: Failing tests (RED) committed first, followed by clean passing implementation (GREEN).
- **Workspace Compilation**: All-features, `--locked`, `--offline` compatibility in Docker container `hippocamp-task4-rust:local`.

---

## 4. Acceptance Criteria

1. `crates/engine` registered in root `Cargo.toml`.
2. Unit and integration tests verify:
   - `inference_deadline_miss_closes_generation_and_outputs_silence`
   - `bypass_keeps_framing_without_calling_inference_or_raw_fallback`
   - `backpressure_drops_frame_and_emits_discontinuity`
   - `generation_restart_warms_replacement_off_thread`
3. 100% test pass offline in Docker with zero warnings (`-D warnings`).
