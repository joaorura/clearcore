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
    "total_crashes": 0
  }
  ```

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
