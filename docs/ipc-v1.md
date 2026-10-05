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
- **Malformed requests:** a line that is not a valid request gets `InvalidCommand` with code `JSON_PARSE_ERROR` and the fixed message `"malformed request"` (the `request_id` is `"unknown"`). The parser error is never echoed. A line that is not valid UTF-8 gets the same answer (`JSON_PARSE_ERROR`, message `"malformed request"`).
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
    "voice_profile_error": null,
    "voice_profile_supported": true,
    "neural_eq_calibrated": true
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
  | `voice_profile_supported` | `true` when the active backend can apply a voice profile at all. `false` on backends that reject every profile (OpenVINO NPU/GPU/CPU, DirectML, RyzenAI, TensorRT, Vulkan, CoreML, the mock tract) and on tract when the loaded model has no `FiLM` (`gamma`/`beta`) inputs, which is the case of the approved base DFNet3; `false` without a backend. While `false`, `BuildVoiceProfile` answers `ENROLL_BACKEND_UNSUPPORTED` immediately. Clients that do not find the field must treat support as unknown. |
  | `neural_eq_calibrated` | `true` when the **applied** profile (`active_voice_profile_id`) carries a microphone EQ; `false` without an applied profile or when it has no EQ. |

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

Status: **development-integrated.** The pipeline runs on the service backend, but the packaged virtual microphone (`filter-capi` / helper) does not apply the profile yet, and there is no claim of improved isolation. The enrollment model is a development asset (section 3.1.7), not a distributed one.

Overview: the app sends a recording with `AddVoiceSample`; the service denoises it with the base DFNet3 in memory, measures its quality, stores the **denoised** WAV (16-bit, 48 kHz, mono) and answers through a job. `BuildVoiceProfile` then joins the active samples of one microphone into a profile and applies it through the `SetVoiceProfile` transaction. The service never writes raw audio to disk (section 3.1.8).

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

- **Response Payload (immediate):** `{"success": true, "job_id": "sample-job-1"}`. Sample ingestion jobs use the `sample-job-N` id format. The final result is read with `GetEnrollmentJob`.
- **Errors (immediate):** `INVALID_COMMAND` (status `InvalidCommand`, message `invalid sample metadata`) when `name` is empty or `name`, `device_label` or `device_id_hash` exceed 256 bytes; `ENROLL_INVALID_AUDIO` (a `sample_rate` other than `48000`, bad base64, empty or misaligned PCM); `ENROLL_PAYLOAD_TOO_LARGE` (more than 90 s of PCM); `ENROLL_BUSY` (two sample jobs already running; status `InternalError`); `ENROLL_MODEL_NOT_CONFIGURED` (status `InternalError`) when the service has no base denoiser (isolation model) available: the request is refused before the audio is decoded and nothing is stored. Errors found during processing (`ENROLL_CLIPPING`, `ENROLL_TOO_QUIET`, `ENROLL_BUDGET_EXCEEDED`, `ENROLL_FAILED`) arrive as the `failed` state of the job.
- **Rules:** a raw peak of `0.99` or more is rejected as clipping; an RMS below -40 dBFS is rejected as too quiet; if the microphone's used speech plus the sample's speech would exceed 90 s (section 3.1.5), nothing is stored and the job fails with `ENROLL_BUDGET_EXCEEDED` carrying `remaining_seconds`.

#### 3.1.2 `BuildVoiceProfile`
Builds a profile from the active samples of the **most recent sample's microphone** and applies it. Samples of other microphones stay in the gallery, flagged `other_microphone`, and are not used. Speech is trimmed (20 ms frames, 40 ms margin), levels are matched to the group median (gain limited to +/-12 dB), segments are joined with a 20 ms crossfade, the microphone EQ is estimated, the audio is resampled 48 to 16 kHz and enrolled with the development model.

- **Request:** `{"BuildVoiceProfile":{"name":"My voice"}}`
- **Response Payload (immediate):** `{"success": true, "job_id": "profile-job-1"}`. Profile build jobs use the `profile-job-N` id format.
- **Immediate errors:** `ENROLL_BACKEND_UNSUPPORTED` (the active backend cannot apply a voice profile, see `voice_profile_supported` in `GetStatus`; checked first after the name, before any audio is read; no job is created and samples are kept), `ENROLL_TOO_LITTLE_SPEECH` (no eligible sample group, or under 6 s of speech in it: record more with this microphone; no job is created), `ENROLL_BUSY` (two profile jobs already running; no job is created), `INVALID_COMMAND` for an invalid `name` (empty or over 256 bytes).
- **Job errors:** `ENROLL_MODEL_NOT_CONFIGURED` (the development model is loaded by the job: variables unset, archive missing, unreadable, oversized, or SHA-256 mismatch, stage `enroll`), `ENROLL_BUDGET_EXCEEDED` (more than 90 s found, e.g. data from before the budget; the builder never discards audio on its own), `ENROLL_BACKEND_UNSUPPORTED` (stage `apply`: the backend passed the up-front check but refused the built profile; the previous profile is kept), `ENROLL_FAILED` (including a sample WAV that cannot be read back, and stage `apply` when the profile could not be persisted or no profile store is attached).
- **Input:** the build reads only `samples/<id>.wav` inside the profile store (ids are validated); the `audio_path` stored in a manifest is never used.

#### 3.1.3 `GetEnrollmentJob`
Reads the state of a job started by `AddVoiceSample` or `BuildVoiceProfile`.

- **Request:** `{"GetEnrollmentJob":{"job_id":"sample-job-1"}}`. The service looks the id up in the sample table (`sample-job-N`) and in the profile table (`profile-job-N`); the ids are no longer `job-N`.
- **Response Payload:**
  ```json
  {
    "job_id": "sample-job-1",
    "state": "done",
    "stage": "denoise",
    "error_code": null,
    "remaining_seconds": null,
    "sample_id": "s-1760000000000-1",
    "profile_id": null,
    "take_id": null,
    "recorded": null,
    "reason": null,
    "quality": {"peak": 0.42, "rms_dbfs": -23.1, "active_fraction": 0.71, "speech_seconds": 8.4}
  }
  ```
  All keys are always present; unused ones are `null`.

  | Field | Meaning |
  | :--- | :--- |
  | `job_id` | The job id. |
  | `state` | `running`, `done` or `failed`. |
  | `stage` | Where the job is or stopped: `denoise` (sample jobs), `trim` (build jobs; also where a build reports `ENROLL_BUDGET_EXCEEDED` or an unreadable WAV), `eq`, `enroll`, `apply`, or `timeout` (a job still running after 10 minutes, failed by the watchdog with `ENROLL_FAILED`; see section 3.1.4). The stage of a running job is its starting stage; it is not updated while it runs. An over-load request is refused immediately with `ENROLL_BUSY` rather than as a `queued` job. |
  | `error_code` | Fixed `ENROLL_*` code when `state` is `failed`, otherwise `null`. |
  | `remaining_seconds` | Budget left; present on `ENROLL_BUDGET_EXCEEDED`. |
  | `sample_id` | Id of the stored sample (`AddVoiceSample` jobs, when done). |
  | `take_id` | Id of the stored call take (`AddIntakeSuggestion` jobs, when recorded). |
  | `recorded` | `AddIntakeSuggestion` jobs only: `true` when the take was stored, `false` when it was skipped. |
  | `reason` | `"budget"` when `recorded` is `false`, otherwise `null`. |
  | `profile_id` | Id of the applied profile (build jobs, when done). |
  | `quality` | `peak` (raw), `rms_dbfs` and `active_fraction` (denoised), `speech_seconds` (active speech after trimming). Sample and take jobs once applied, otherwise `null`. |

  An unknown or expired job id (or one without a valid prefix) returns `ENROLL_JOB_NOT_FOUND`. Each table keeps only the 64 most recent finished jobs.

#### 3.1.4 Job lifecycle
- `AddVoiceSample`, `AddIntakeSuggestion` and `BuildVoiceProfile` answer immediately with a `job_id` (`sample-job-N` or `profile-job-N`); the work runs on a worker thread.
- At most **2 jobs run at the same time per job table** (`MAX_RUNNING_JOBS`): two in the sample table (`AddVoiceSample` and `AddIntakeSuggestion` together) and two in the profile table (`BuildVoiceProfile`), so up to four in total. A further request to a full table is refused immediately with `ENROLL_BUSY`; no job is created. Retry after a job finishes.
- **Watchdog:** a job still `running` 10 minutes after it started becomes `failed` with `ENROLL_FAILED` and stage `timeout`, which frees its slot. The worker thread cannot be killed; its late result is discarded.
- A finished job does not change service state by itself: its result is **applied on the daemon thread at the start of the NEXT request** (any request, e.g. the periodic `GetStatus`). The client must therefore call `GetEnrollmentJob` periodically until `state` leaves `running`; the poll itself triggers the application. A job that finishes while no client is talking to the service waits for the next request.
- The profile of a build job is applied through the `SetVoiceProfile` transaction (apply to the backend, persist atomically, roll back on failure).
- Connection limits: the service serves **one client at a time**. Every accepted connection has a 30 s read and write timeout; a peer that stays idle longer, or never finishes a line, has its session ended (the daemon keeps accepting). A request line is read with a cap of 32 MiB; a longer line is discarded up to its newline and answered with `ENROLL_PAYLOAD_TOO_LARGE` (status `InvalidCommand`, request id `unknown`), and the connection stays usable.

#### 3.1.5 Changed commands and the speech budget
- **`ListVoiceSamples`:** `audio_path` is **not exposed** (neither for the stored WAV nor for legacy samples). Reply:
  ```json
  {
    "samples": [
      {"id": "s-1760000000000-1", "name": "Take 1", "timestamp": "1760000000000", "speech_seconds": 8.4,
       "device_label": "Built-in Microphone", "device_id_hash": "3f2a...", "is_active": true,
       "needs_reenroll": false, "used_in_profile": true, "other_microphone": false}
    ],
    "total_count": 1,
    "has_profile": false,
    "selected_device_id_hash": "3f2a...",
    "selected_device_label": "Built-in Microphone",
    "budget": {"used_seconds": 8.4, "max_seconds": 90.0, "remaining_seconds": 81.6}
  }
  ```
  `needs_reenroll` marks a migrated sample with no WAV (counts 0 s and must be recorded again). `other_microphone` marks a sample of a microphone other than the selected group (not used). The selected group is the `device_id_hash` of the most recent active sample that has audio; `selected_device_id_hash` is that hash and `selected_device_label` the label of its most recent sample, both `null` when no sample is eligible. `used_seconds` is the sum of `speech_seconds` of the active samples with audio in the selected group; with no eligible sample the budget is empty (`0` used, `90` remaining).
- **`AddIntakeSuggestion`:** `take_json` is a JSON **string** with the same audio as `AddVoiceSample`: `{"pcm_f32_le_b64":"<base64>","sample_rate":48000,"device_label":"...","device_id_hash":"...","name":"optional"}` (unknown fields are rejected; `device_label`, `device_id_hash` and `name` are optional). The old JSON form without PCM is refused with `INVALID_COMMAND` (`Invalid intake suggestion payload`). Audio errors are the immediate ones of `AddVoiceSample` (`ENROLL_INVALID_AUDIO`, `ENROLL_PAYLOAD_TOO_LARGE`, `ENROLL_BUSY`, and `ENROLL_MODEL_NOT_CONFIGURED` without a base denoiser). The reply is `{"success": true, "job_id": "sample-job-N"}`; the take is denoised like a sample and the outcome is read with `GetEnrollmentJob`: `recorded: true` with `take_id`, or `recorded: false, reason: "budget"` with `state: "done"` and **no error** when the microphone's `remaining_seconds` is under 5 s or the take's speech does not fit in it (nothing is stored). A budget skip is never a `failed` job.
- **`ListIntakeSuggestions`:** items carry `id`, `timestamp`, `duration_secs`, `snr`, `speech_seconds` and `device_label`; the audio path is not exposed. The reply adds `count`.
- **`ApproveIntakeSuggestion`:** rechecks the budget of the take's microphone before touching the take. If the take no longer fits, the call is refused with `ENROLL_BUDGET_EXCEEDED` (status `InvalidCommand`) and **the take is kept** so the user can free budget and approve again. On success: `{"success": true, "approved": true, "sample_id": "...", "has_profile": false}`. An unknown id returns `TAKE_NOT_FOUND`.
- **`DeleteVoiceSample`:** unchanged request; also removes the WAV. The client triggers a rebuild if it wants one. Only the user frees budget; nothing is deleted automatically.
- **`GetVoiceProfileEmbedding`:** **deprecated** (it exposed a placeholder embedding). It always answers the fixed error `DEPRECATED` (status `InvalidCommand`, message `deprecated`) until removed.
- **Legacy samples:** the gallery from before the pipeline is migrated once, in a single pass, into the profile store directory when the daemon starts. Migrated samples carry no audio and appear as `needs_reenroll`; a failed migration only logs a fixed line and never stops the daemon.

#### 3.1.6 Error codes
Codes are fixed strings with no free text and no echo of the payload (`crates/ipc/src/enrollment_codes.rs`).

| Code | Meaning |
| :--- | :--- |
| `ENROLL_CLIPPING` | The recording clips (raw peak at or above 0.99). Re-record at a lower input level. |
| `ENROLL_TOO_QUIET` | The denoised speech is quieter than -40 dBFS RMS. |
| `ENROLL_TOO_LITTLE_SPEECH` | Less than 6 s of speech in the recording group (or no group). Immediate response of `BuildVoiceProfile`; record more with the same microphone. |
| `ENROLL_MODEL_NOT_CONFIGURED` | A model the pipeline needs is not available: the development enrollment model is absent, unreadable, oversized or its SHA-256 does not match (see below; reported by the build job, stage `enroll`), or the service has no base denoiser (isolation model) available (immediate response of `AddVoiceSample` and `AddIntakeSuggestion`). |
| `ENROLL_BUDGET_EXCEEDED` | The microphone would exceed 90 s of speech. Delete audio first. |
| `ENROLL_INVALID_AUDIO` | The audio payload is not valid (bad base64, wrong length, non-finite samples, unsupported sample rate). |
| `ENROLL_PAYLOAD_TOO_LARGE` | The request line exceeded 32 MiB (`MAX_REQUEST_LINE_BYTES`), or the PCM exceeded 90 s. |
| `ENROLL_JOB_NOT_FOUND` | Unknown or expired `job_id`. |
| `ENROLL_FAILED` | Generic failure of the pipeline (denoise, enrollment, persistence), including a job failed by the 10-minute watchdog (stage `timeout`). No detail is exposed. |
| `ENROLL_BUSY` | Two enrollment jobs are already running in that table; immediate response, no job is created; retry later. |
| `ENROLL_BACKEND_UNSUPPORTED` | The active isolation model cannot apply a voice profile (status `InternalError`). Immediate response of `BuildVoiceProfile` when `voice_profile_supported` is `false` (no job is created), or the `apply` stage of a build job when the backend refused the built profile. Samples and earlier profiles are kept. |

#### 3.1.7 Development model configuration
The enrollment model is loaded only when both environment variables are set for the service:

| Variable | Value |
| :--- | :--- |
| `CLEARCORE_DEV_ENROLLMENT_ASSET` | Absolute path of the asset archive (`voice-enrollment-asset-v1.tar.gz`). |
| `CLEARCORE_DEV_ENROLLMENT_SHA256` | Expected SHA-256 of the **entire file**: 64 lowercase hex characters. |

The hash is checked over the whole file and only an allowlisted archive member (`enrollment.onnx`) is read. Without a valid configuration (variables unset, archive missing, unreadable or oversized, or a hash mismatch), `BuildVoiceProfile` jobs fail with `ENROLL_MODEL_NOT_CONFIGURED`; sample ingestion does not need the enrollment model (it needs only the base denoiser). The asset is not pinned in the registry, not signed and not distributed. Denoising uses the approved base DFNet3. Scope: development-integrated; the packaged virtual microphone does not apply the profile yet, and no improvement of voice isolation is claimed.

#### 3.1.8 Privacy
- Raw PCM exists only in memory and is zeroed after denoising; **the service never writes raw audio to disk**. The stored WAV is the **denoised** one.
- Core dumps of the service are disabled in the supported launches (they would copy the in-memory PCM to disk): the systemd user unit sets `LimitCORE=0`, and the Electron app, `start-realtime-noise.sh` and `dev.sh` start the daemon with `RLIMIT_CORE=0` (`prlimit --core=0`, or `ulimit -c 0` where `prlimit` is missing). A daemon started by hand, or a system-wide crash handler that overrides the limit, is outside this guarantee.
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
