# Diagnostic Schema & Privacy Guarantees

This document specifies the privacy contract, schema definition, and export guarantees for Project Hippocamp diagnostic archives (`realtime-noise.diagnostics.v1`).

---

## 1. Privacy Contract & Guarantees

Project Hippocamp implements an uncompromising privacy-first design for all diagnostics and telemetry. In compliance with strict user privacy mandates, diagnostic data is collected solely for fault analysis, latency tracking, and operational stability.

### Strict Prohibitions
1. **Zero Raw Audio Leakage**: Diagnostic payloads and archives NEVER embed raw PCM audio samples, float buffers, or audio waveforms.
2. **Zero Embedding Vector Leakage**: High-dimensional ML embedding vectors (e.g. speaker embeddings, d-vectors, or neural latent representations) are strictly excluded from all exports.
3. **Zero Meeting & Transcript Leakage**: Meeting titles, calendar events, call participant names, and speech recognition transcripts are never included. Text sanitizers actively redact quoted expressions and sensitive conversational keywords.
4. **Salted Per-Installation Device Hashing**: Audio endpoint identifiers and hardware serials are hashed using SHA-256 combined with a local per-installation cryptographic salt. Raw device IDs, manufacturer serial numbers, and OS paths are never exported.
5. **Filesystem Anonymization**: Operating system usernames and directory paths are redacted (e.g. `/home/[REDACTED_USER]/` or `C:\Users\[REDACTED_USER]\`).
6. **Fail-Closed Envelope Verification**: The exporter performs an automated inspection of outgoing serialized bytes before emission. If forbidden keys (`pcm_samples`, `embeddings`, `meeting_title`, etc.) are detected, export immediately fails closed.

---

## 2. Schema Specification: `realtime-noise.diagnostics.v1`

### Data Structure (`DiagnosticPayload`)

| Field | Type | Description |
|---|---|---|
| `schema_version` | `string` | Version identifier: `"realtime-noise.diagnostics.v1"` |
| `device_id_hash` | `string` (hex 64) | SHA-256 salted hash of input device identifier |
| `generation` | `u64` | Supervisor engine restart / lifecycle generation counter |
| `causes` | `array<string>` | Sanitized crash reasons, backoff triggers, or state change logs |
| `last_attempt` | `optional<usize>` | Index of the last retry attempt in current backoff window |
| `latencies` | `LatencyPercentiles` | Latency distribution metrics (microseconds) |
| `budget` | `BudgetBreakdown` | Audio frame budget allocation (microseconds) |
| `timestamp_utc` | `string` | Export generation timestamp |

### Substructures

#### `LatencyPercentiles`
- `p50_us` (`u64`): Median processing latency across measured hops.
- `p95_us` (`u64`): 95th percentile latency.
- `p99_us` (`u64`): 99th percentile tail latency.

#### `BudgetBreakdown`
- `measured_us` (`u64`): Measured DSP execution and inference time.
- `configured_us` (`u64`): Allocated budget ceiling for audio hop duration.
- `derived_us` (`u64`): Derived buffer and format adaptation overhead.
- `unobservable_us` (`u64`): Unobservable OS kernel/hardware DMA jitter estimate.

---

## 3. Example Export Payload

```json
{
  "schema_version": "realtime-noise.diagnostics.v1",
  "device_id_hash": "a8f5b4c3e2109876543210fedcba9876543210fedcba9876543210fedcba9876",
  "generation": 42,
  "causes": [
    "Crash: buffer overrun during [REDACTED_CONTENT] session",
    "Supervisor restart after backoff delay (attempt 2)"
  ],
  "last_attempt": 3,
  "latencies": {
    "p50_us": 1200,
    "p95_us": 2800,
    "p99_us": 4500
  },
  "budget": {
    "measured_us": 1500,
    "configured_us": 3000,
    "derived_us": 800,
    "unobservable_us": 200
  },
  "timestamp_utc": "1727736000s-epoch"
}
```

---

## 4. Verification

The diagnostic export guarantees are automatically validated in continuous integration via:
```bash
cargo test -p realtime-noise-diagnostics --locked --offline
```
This test asserts that raw PCM vectors, embedding vectors, unhashed device identifiers, transcripts, and meeting titles are strictly purged from exported archives.
