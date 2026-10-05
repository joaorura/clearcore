# Local Control-Plane IPC Protocol (`realtime-noise.v1`)

## 1. Overview & Architectural Principles

The Project Hippocamp control plane operates via a local inter-process communication (IPC) channel (`crates/ipc`) connecting user-facing configuration frontends (CLI, systray app, settings UI) to the background daemon (`realtime-noise-service`).

Key architectural guarantees:
1. **Control-Plane Only (Zero PCM Audio):** Raw audio data is strictly forbidden over the IPC channel. Audio stays entirely within the real-time audio pipeline and lock-free ring buffers (`crates/contracts`, `crates/engine`).
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
| `request_id` | `String` | Unique client request identifier, echoed in the response. |
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
- `VersionMismatch`: Client sent an incompatible `version` string. Connection fail-closed.
- `Unauthorized`: Client does not have permissions to execute the command.
- `InvalidCommand`: Malformed JSON or unparseable command structure.
- `InternalError`: Internal error during command processing.

---

## 5. Security & Isolation Boundary

- The IPC endpoint is restricted to the current user's session.
- On Unix, file permissions on the domain socket are set to `0600` (owner read/write only).
- On Windows, the named pipe ACL restricts access to the current logon token (`RunLevel Limited`).
