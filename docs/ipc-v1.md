# Local Control-Plane IPC Protocol (`realtime-noise.v1`)

## 1. Overview & Architectural Principles

The Project Hippocamp control plane operates via a local inter-process communication (IPC) channel (`crates/ipc`) connecting user-facing configuration frontends (CLI, systray app, settings UI) to the background daemon (`realtime-noise-service`).

Key architectural guarantees:
1. **Control-Plane Only (No Real-Time Audio):** The real-time audio stream is strictly forbidden over the IPC channel. Real-time audio stays entirely within the audio pipeline and lock-free ring buffers (`crates/contracts`, `crates/engine`). The single exception is voice enrollment: `AddVoiceSample` and `AddIntakeSuggestion` carry one bounded, base64-encoded recording (section 3.1) that the service denoises in memory and never writes to disk raw.
2. **Strict Protocol Versioning:** The protocol is version-tagged (`realtime-noise.v1`). Incompatible clients are rejected fail-closed with `VersionMismatch`.
3. **Session Decoupling & Safe Disconnect:** The daemon outlives client sessions. Frontends may disconnect, terminate, or reconnect arbitrarily without interrupting audio processing or engine state.
4. **Platform Transport:**
   - **Linux / macOS:** Local Unix domain stream socket (default `$XDG_RUNTIME_DIR/realtime-noise.sock` or `/tmp/realtime-noise.sock`).
   - **Windows:** Named pipe (`\\.\pipe\realtime-noise`).

---

## 2. Framing & Envelope Specification

Messages are formatted as single-line JSON records delimited by a newline (`\n`).

### 2.1 Request Envelope (`IpcRequest`)

```json
{
  "version": "realtime-noise.v1",
  "request_id": "req-1727726400000000",
  "command": "GetStatus",
  "payload": {}
}
```

| Field | Type | Description |
|---|---|---|
| `version` | `String` | Protocol version; must exactly match `"realtime-noise.v1"`. |
| `request_id` | `String` | Unique client request identifier, echoed in the response truncated to 64 characters (`MAX_ECHOED_REQUEST_ID_CHARS`). |
| `command` | `IpcCommand` | The requested command variant. |
| `payload` | `Value` | Command parameters or metadata (JSON object/null). |

### 2.2 Response Envelope (`IpcResponse`)

```json
{
  "version": "realtime-noise.v1",
  "request_id": "req-1727726400000000",
  "status": "Ok",
  "payload": {
    "state": "Running",
    "mode": "Active",
    "crash_count_15m": 0,
    "total_crashes": 0
  },
  "error": null
}
```

| Field | Type | Description |
|---|---|---|
| `version` | `String` | Protocol version (`"realtime-noise.v1"`). |
| `request_id` | `String` | Matches the `request_id` of the originating request. |
| `status` | `IpcStatus` | Execution status code. |
| `payload` | `Value` | Command output data or `{}`. |
| `error` | `Option<IpcErrorDetail>` | Present on error with `code` and `message`. |

### 2.3 Request line limit and fixed error messages

- **Line cap:** a request line (including its `\n`) may be at most `MAX_REQUEST_LINE_BYTES` = 32 MiB. The cap fits the largest enrollment payload (90 s x 48 kHz x 4 B = 17.28 MB of PCM, 23.04 MB after base64) plus the envelope. The reader never buffers more than the cap; a longer line is discarded up to its `\n` so the connection stays in sync, and the response is `InvalidCommand` with code `ENROLL_PAYLOAD_TOO_LARGE`.
- **Malformed requests:** a line that is not a valid request gets `InvalidCommand` with code `JSON_PARSE_ERROR` and the fixed message `"malformed request"` (the `request_id` is `"unknown"`). The parser error is never echoed. A line that is not valid UTF-8 gets the fixed message `"request line is not valid UTF-8"`.
- **No echo of client data:** `VERSION_MISMATCH` carries a fixed message and does not repeat the `version` the client sent. Error messages in general never echo request content (voice profile, audio, ids).
- A transport error on the connection ends that session; the daemon keeps running.

---

## 3. Command Definitions (`IpcCommand`)

### `GetStatus`
Retrieves current supervisor lifecycle state, operational mode, and crash telemetry.

- **Request:**
  ```json
  {"version":"realtime-noise.v1","request_id":"c1","command":"GetStatus","payload":{}}
  ```
- **Response Payload:**
  ```json
  {
    "state": "Running",
    "is_terminal": false,
    "can_restart": true,
    "mode": "Active",
    "crash_count_15m": 0,
    "total_crashes": 0,
    "active_voice_profile_id": "spk-1",
    "stored_voice_profile_id": "spk-1",
    "voice_profile_selected": true,
    "is_voice_profile_active": true,
    "voice_profile_error": null
  }
  ```
  (The payload also carries preset, backend and voice-sample fields, omitted here.)

  Voice profile fields:

  | Field | Meaning |
  | :--- | :--- |
  | `active_voice_profile_id` | Id of the profile **applied** on the service's backend, or `null`. Since this change it no longer means "stored on disk": a stored profile the backend rejected is reported as `null`. |
  | `stored_voice_profile_id` | Id of the profile persisted on disk (the one reloaded at startup and after a backend swap), or `null`. |
  | `voice_profile_selected` | `true` when `stored_voice_profile_id` is not `null`. |
  | `is_voice_profile_active` | `true` when `active_voice_profile_id` is not `null`. |
  | `voice_profile_error` | Generic, constant reason the stored profile is not applied (backend rejected it, file missing/unreadable, rollback failed), or `null`. Never carries profile data or backend error text. |

  Scope: "applied" means applied on the **service's own** backend. The packaged audio path
  (`filter-capi` / helper / virtual microphone) does not use the profile yet (stage 2). The
  fields are reported in every mode, including `Mute`, `Bypass` and `TerminalSafeState`.

### `SetVoiceProfile`
Applies a voice profile to the service backend, then persists it. The operation is transactional:
the profile is verified, applied to the backend and saved; if saving fails the backend returns to
the previous profile.

- **Request:**
  ```json
  {"version":"realtime-noise.v1","request_id":"c6","command":{"SetVoiceProfile":{"profile_json":"{...}"}},"payload":{}}
  ```
- **Response Payload:**
  ```json
  {"active_voice_profile_id": "spk-1"}
  ```
- **Errors:**

  | Status | Code | Cause |
  | :--- | :--- | :--- |
  | `InvalidCommand` | `INVALID_COMMAND` | `profile_json` is not a valid voice profile (the message never echoes the input). |
  | `InternalError` | `NO_PROFILE_STORE` | The service has no profile storage configured. |
  | `InternalError` | `VOICE_PROFILE_NOT_APPLICABLE` | The active backend cannot apply this profile; nothing is persisted. |
  | `InternalError` | `INTERNAL_ERROR` | Persisting failed (`Failed to persist voice profile`); the backend was rolled back. |

### `ClearVoiceProfile`
Returns the backend to the neutral voice and removes the stored profile. Idempotent.

- **Request:**
  ```json
  {"version":"realtime-noise.v1","request_id":"c7","command":"ClearVoiceProfile","payload":{}}
  ```
- **Response Payload:**
  ```json
  {"active_voice_profile_id": null}
  ```
- **Errors:**

  | Status | Code | Cause |
  | :--- | :--- | :--- |
  | `InternalError` | `VOICE_PROFILE_CLEAR_FAILED` | The backend could not go neutral; disk and reported state are left untouched. |
  | `InternalError` | `INTERNAL_ERROR` | Removing the stored file failed; the backend was restored. |

### 3.1 Voice enrollment

Status: **development-integrated.** The pipeline runs on the service backend, but the packaged virtual microphone (`filter-capi` / helper) does not apply the profile yet, and there is no claim of improved isolation. The enrollment model is a development asset (section 3.1.6), not a distributed one.

Overview: the app sends a recording with `AddVoiceSample`; the service denoises it with the base DFNet3 in memory, measures its quality, stores the **denoised** WAV (16-bit, 48 kHz, mono) and answers through a job. `BuildVoiceProfile` then joins the active samples of one microphone into a profile and applies it through the `SetVoiceProfile` transaction. Raw audio is never written to disk.

#### 3.1.1 `AddVoiceSample`
Stores one recording as a gallery sample. Replaces the previous embedding-carrying form.

- **Request:**
  ```json
  {"version":"realtime-noise.v1","request_id":"e1","command":{"AddVoiceSample":{"name":"Take 1","pcm_f32_le_b64":"<base64>","sample_rate":48000,"device_label":"Built-in Microphone","device_id_hash":"3f2a..."}},"payload":{}}
  ```

  | Field | Meaning |
  | :--- | :--- |
  | `name` | Display name of the sample. |
  | `pcm_f32_le_b64` | Base64 of little-endian `f32` PCM, mono. Subject to the 32 MiB line cap. |
  | `sample_rate` | Capture sample rate; the pipeline expects `48000`. |
  | `device_label` | Display label of the physical capture device. |
  | `device_id_hash` | Stable hash of the device identifier, computed by the app. Identifies the microphone group used by the budget and the profile build. |

- **Response Payload (immediate):** `{"job_id": "..."}`. The final result is read with `GetEnrollmentJob`.
- **Errors (immediate):** `ENROLL_PAYLOAD_TOO_LARGE`, `ENROLL_INVALID_AUDIO`, `ENROLL_MODEL_NOT_CONFIGURED`, `ENROLL_BUSY`. Errors found during processing (`ENROLL_CLIPPING`, `ENROLL_TOO_QUIET`, `ENROLL_BUDGET_EXCEEDED`, `ENROLL_FAILED`) arrive as the `failed` state of the job.
- **Rules:** a raw peak of `0.99` or more is rejected as clipping; an RMS below -40 dBFS is rejected as too quiet; if the microphone's used speech plus the sample's speech would exceed 90 s (section 3.1.5), nothing is stored and the job fails with `ENROLL_BUDGET_EXCEEDED` carrying `remaining_seconds`.

#### 3.1.2 `BuildVoiceProfile`
Builds a profile from the active samples of the **most recent sample's microphone** and applies it. Samples of other microphones stay in the gallery, flagged `other_microphone`, and are not used. Speech is trimmed (20 ms frames, 40 ms margin), levels are matched to the group median (gain limited to +/-12 dB), segments are joined with a 20 ms crossfade, the microphone EQ is estimated, the audio is resampled 48 to 16 kHz and enrolled with the development model.

- **Request:** `{"BuildVoiceProfile":{"name":"My voice"}}`
- **Response Payload (immediate):** `{"job_id": "..."}`.
- **Job errors:** `ENROLL_TOO_LITTLE_SPEECH` (under 6 s of speech in the group: record more with this microphone), `ENROLL_BUDGET_EXCEEDED` (more than 90 s found, e.g. data from before the budget; the builder never discards audio on its own), `ENROLL_MODEL_NOT_CONFIGURED`, `ENROLL_FAILED`.

#### 3.1.3 `GetEnrollmentJob`
Reads the state of a job started by `AddVoiceSample` or `BuildVoiceProfile`.

- **Request:** `{"GetEnrollmentJob":{"job_id":"..."}}`
- **Response Payload:**
  ```json
  {
    "job_id": "job-1",
    "state": "done",
    "stage": "apply",
    "error_code": null,
    "remaining_seconds": null,
    "sample_id": "s-1",
    "profile_id": null,
    "quality": {"peak": 0.42, "rms_dbfs": -23.1, "active_fraction": 0.71, "speech_seconds": 8.4}
  }
  ```

  | Field | Meaning |
  | :--- | :--- |
  | `job_id` | The job id. |
  | `state` | `running`, `done` or `failed`. |
  | `stage` | Where the job is or stopped: `denoise`, `trim`, `eq`, `enroll`, `apply`. A job refused for load reports `queued`; a job that exceeded its time limit reports `timeout`. |
  | `error_code` | Fixed `ENROLL_*` code when `state` is `failed`, otherwise `null`. |
  | `remaining_seconds` | Budget left; present on `ENROLL_BUDGET_EXCEEDED`. |
  | `sample_id` | Id of the stored sample (sample jobs, when done). |
  | `profile_id` | Id of the applied profile (build jobs, when done). |
  | `quality` | `peak` (raw), `rms_dbfs` and `active_fraction` (denoised), `speech_seconds` (active speech after trimming). Sample jobs only. |

  An unknown or expired job id returns `ENROLL_JOB_NOT_FOUND`. The service keeps only the most recent finished jobs.

  Note (contract): the exact `quality` and `sample_id` / `profile_id` field placement follows the specification and is finalized by the service handler (task S6).

#### 3.1.4 Job lifecycle
- `AddVoiceSample` and `BuildVoiceProfile` answer immediately with a `job_id`; the work runs on a worker thread.
- At most **2 jobs run at the same time**. A third request is refused with `ENROLL_BUSY` (stage `queued`); retry after a job finishes.
- A finished job does not change service state by itself: its result is **applied on the daemon thread at the start of the NEXT request** (any request, e.g. the periodic `GetStatus`). The client must therefore call `GetEnrollmentJob` periodically until `state` leaves `running`; the poll itself triggers the application. A job that finishes while no client is talking to the service waits for the next request.
- The profile of a build job is applied through the `SetVoiceProfile` transaction (apply to the backend, persist atomically, roll back on failure).

#### 3.1.5 Changed commands and the speech budget
- **`ListVoiceSamples`:** `audio_path` now points to the stored WAV. Each sample adds `speech_seconds`, `device_label`, `used_in_profile`, `needs_reenroll` (migrated sample with no WAV; counts 0 s and must be recorded again) and `other_microphone` (sample of a microphone other than the current group; not used). The reply adds:
  ```json
  {"budget": {"used_seconds": 42.5, "max_seconds": 90, "remaining_seconds": 47.5}}
  ```
  The budget is evaluated for the current microphone group: `used_seconds` is the sum of `speech_seconds` of its active samples.
- **`AddIntakeSuggestion` / `ApproveIntakeSuggestion`:** same commands, now carrying audio like `AddVoiceSample` and gated by the budget. A call take (`AddIntakeSuggestion`) is recorded only if `remaining_seconds` is at least 5 s **and** the take's speech fits in `remaining_seconds`; otherwise it is **not stored and no error is returned** (the reply says `recorded: false, reason: "budget"`). `ApproveIntakeSuggestion` rechecks the budget; if the take no longer fits it is refused with `ENROLL_BUDGET_EXCEEDED` and **the take is kept** so the user can free budget and approve again.
- **`DeleteVoiceSample`:** unchanged request; also removes the WAV. The client triggers a rebuild if it wants one. Only the user frees budget; nothing is deleted automatically.
- **`GetVoiceProfileEmbedding`:** **deprecated** (it exposed a placeholder embedding). It keeps returning an error until removed.

Note (contract): the `recorded` / `reason` reply of `AddIntakeSuggestion` and the budget fields above follow the specification (section 4.4 and 5); the handler is finalized in task S6.

#### 3.1.6 Error codes
Codes are fixed strings with no free text and no echo of the payload (`crates/ipc/src/enrollment_codes.rs`).

| Code | Meaning |
| :--- | :--- |
| `ENROLL_CLIPPING` | The recording clips (raw peak at or above 0.99). Re-record at a lower input level. |
| `ENROLL_TOO_QUIET` | The denoised speech is quieter than -40 dBFS RMS. |
| `ENROLL_TOO_LITTLE_SPEECH` | Less than 6 s of speech in the recording group; record more with the same microphone. |
| `ENROLL_MODEL_NOT_CONFIGURED` | The development enrollment model is not configured (see below). |
| `ENROLL_BUDGET_EXCEEDED` | The microphone would exceed 90 s of speech. Delete audio first. |
| `ENROLL_INVALID_AUDIO` | The audio payload is not valid (bad base64, wrong length, non-finite samples, unsupported sample rate). |
| `ENROLL_PAYLOAD_TOO_LARGE` | The request line exceeded 32 MiB (`MAX_REQUEST_LINE_BYTES`). |
| `ENROLL_JOB_NOT_FOUND` | Unknown or expired `job_id`. |
| `ENROLL_FAILED` | Generic failure of the pipeline (denoise, enrollment, persistence). No detail is exposed. |
| `ENROLL_BUSY` | Two enrollment jobs are already running; retry later. |

#### 3.1.7 Development model configuration
The enrollment model is loaded only when both environment variables are set for the service:

| Variable | Value |
| :--- | :--- |
| `CLEARCORE_DEV_ENROLLMENT_ASSET` | Absolute path of the asset archive (`voice-enrollment-asset-v1.tar.gz`). |
| `CLEARCORE_DEV_ENROLLMENT_SHA256` | Expected SHA-256 of the **entire file**: 64 lowercase hex characters. |

The hash is checked over the whole file and only an allowlisted archive member (`enrollment.onnx`) is read. Without a valid configuration, enrollment answers `ENROLL_MODEL_NOT_CONFIGURED`. The asset is not pinned in the registry, not signed and not distributed. Denoising uses the approved base DFNet3. Scope: development-integrated; the packaged virtual microphone does not apply the profile yet, and no improvement of voice isolation is claimed.

#### 3.1.8 Privacy
- Raw PCM exists only in memory and is zeroed after denoising; **raw audio never goes to disk**. The stored WAV is the **denoised** one.
- WAVs (`samples/<sample-id>.wav`), the sample manifest and profiles are created with mode `0600` in directories `0700`; symlinks are refused.
- Voice samples, WAVs and profiles are excluded from diagnostics and logs, and are deleted with their sample or when the profile is cleared.

---

### `SetMode`
Changes active suppression mode: `"Active"`, `"Bypass"`, or `"Mute"`.

- **Request:**
  ```json
  {"version":"realtime-noise.v1","request_id":"c2","command":{"SetMode":"Bypass"},"payload":{}}
  ```
- **Response Payload:**
  ```json
  {
    "mode": "Bypass",
    "success": true
  }
  ```

### `RestartGeneration`
Manually resets supervisor error states, clears `TerminalSafeState`, and re-initializes engine worker pipelines.

- **Request:**
  ```json
  {"version":"realtime-noise.v1","request_id":"c3","command":"RestartGeneration","payload":{}}
  ```
- **Response Payload:**
  ```json
  {
    "restarted": true,
    "state": "Running"
  }
  ```

### `GetDiagnostics`
Fetches crash logs and supervisor diagnostic traces.

- **Request:**
  ```json
  {"version":"realtime-noise.v1","request_id":"c4","command":"GetDiagnostics","payload":{}}
  ```
- **Response Payload:**
  ```json
  {
    "diagnostics": [
      "Crash #1: inference timeout"
    ],
    "total_crashes": 1
  }
  ```

### `Shutdown`
Requests graceful daemon shutdown.

- **Request:**
  ```json
  {"version":"realtime-noise.v1","request_id":"c5","command":"Shutdown","payload":{}}
  ```
- **Response Payload:**
  ```json
  {
    "shutdown": true
  }
  ```

---

## 4. Status Codes (`IpcStatus`)

- `Ok`: Request was processed successfully.
- `VersionMismatch`: Client sent an incompatible `version` string (code `VERSION_MISMATCH`, fixed message, no echo of the sent value). Connection fail-closed.
- `Unauthorized`: Client does not have permissions to execute the command.
- `InvalidCommand`: Malformed JSON or unparseable command structure (`JSON_PARSE_ERROR`, message `"malformed request"`), or a request line over 32 MiB (`ENROLL_PAYLOAD_TOO_LARGE`).
- `InternalError`: Internal error during command processing.

---

## 5. Security & Isolation Boundary

- The IPC endpoint is restricted to the current user's session.
- On Unix, file permissions on the domain socket are set to `0600` (owner read/write only).
- On Windows, the named pipe ACL restricts access to the current logon token (`RunLevel Limited`).
