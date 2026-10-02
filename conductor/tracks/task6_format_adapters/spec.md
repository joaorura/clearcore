# Specification: Task 6 Framing, Resampling & External Format Adapters

**Track ID:** `task6_format_adapters`  
**Created:** 2026-09-30  
**Phase:** Wave 2 (Onda 2)  
**Parent Plan:** [`docs/superpowers/plans/2026-09-23-realtime-noise-suppression-plan.md`](../../../docs/superpowers/plans/2026-09-23-realtime-noise-suppression-plan.md#L332-L374)

---

## 1. Overview & Objective

Implement `crates/format-adapter` (`realtime-noise-format-adapter`), providing bounded audio format adapters to bridge arbitrary native hardware callback sizes (e.g. 127, 353, 512, 1024 samples) and sample formats (PCM16, Float32, stereo/mono) with the engine's canonical 48 kHz mono 480-sample (`AudioFrame`) pipeline.

---

## 2. Core Components

1. **`InputAccumulator`:**
   - Accumulates variable-sized input slices from audio capture callbacks into fixed 480-sample 48 kHz mono frames.
   - Handles irregular buffers (e.g., 127 samples followed by 353 samples).
   - Zero reallocation in steady-state loop; uses bounded internal ring buffer.

2. **`OutputDeframer`:**
   - Buffers processed 480-sample frames and yields arbitrary requested chunk sizes to native audio output callbacks.
   - When underrun occurs or engine is in silence/fail-closed mode, outputs digital silence.

3. **`EndpointFormatConverter`:**
   - Converts between sample representations:
     - `f32` normalized [-1.0, 1.0]
     - `i16` PCM16 (required by Windows WaveRT endpoints)
     - Channel downmixing (stereo to mono) and upmixing (mono to stereo).
   - Explicit conversions only; unsupported or malformed formats fail closed rather than making implicit approximations.

4. **`FormatAdapterWorker`:**
   - Decoupled worker connecting native callbacks to `BoundedQueueTransport`.

---

## 3. Non-Functional Requirements

- `#![forbid(unsafe_code)]`
- Zero allocation in audio loop.
- Offline Docker test verification with zero warnings.
