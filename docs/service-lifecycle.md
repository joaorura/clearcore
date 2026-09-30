# Service Lifecycle & Engine Supervision Architecture

## 1. Overview

The Project Hippocamp service layer (`crates/service`) and supervisor (`crates/supervisor`) provide fault-tolerant, continuous execution of real-time audio noise suppression.

The service binary (`realtime-noise-service`) runs as an unprivileged, per-user daemon managed by operating system service managers.

```
+-----------------------------------------------------------+
| OS Service Manager (systemd --user / LaunchAgent / Task)  |
+-----------------------------------------------------------+
                              | spawns & monitors
                              v
             +----------------------------------+
             |     realtime-noise-service       |
             | +------------------------------+ |
             | |       ServiceDaemon          | |
             | | +--------------------------+ | |
             | | |     EngineSupervisor     | | |
             | | | +----------------------+ | | |
             | | | |    BackoffTracker    | | | |
             | | | +----------------------+ | | |
             | | +--------------------------+ | |
             | | +--------------------------+ | |
             | | |       IpcServer          | | |
             | | +--------------------------+ | |
             | +------------------------------+ |
             +----------------------------------+
                              ^
                              | IPC (Unix Domain Socket / Named Pipe)
             +----------------------------------+
             |  Frontend UI / CLI / Systray     |
             +----------------------------------+
```

---

## 2. Supervisor States & State Machine

The `EngineSupervisor` tracks the operational health of the noise suppression engine:

```
               [ Engine Startup ]
                       |
                       v
                +-------------+
        +------>|   Running   |<------+
        |       +-------------+       |
        |              |              |
        |        Crash detected       |
        |              |              |
        |              v              |
        |       +-------------+       |
        |       | Restarting  |       |
        |       +-------------+       |
        |              |              |
        |       delay elapsed         |
        +--------------+              |
                       |              |
              6th crash in 15 mins    | Explicit IPC
                       |              | reset command
                       v              |
              +------------------+    |
              | TerminalSafeState|----+
              +------------------+
```

### State Definitions

1. `Running`:
   Engine actively processes real-time audio hops. IPC queries return status `Running`.
2. `EngineUnavailable`:
   Audio devices or ONNX/tract models are temporarily absent. Audio endpoint outputs digital silence.
3. `Restarting { attempt: usize, next_retry_ms: u64 }`:
   A transient failure occurred. The supervisor waits for the prescribed exponential backoff delay before restarting the engine worker thread.
4. `TerminalSafeState { reason: String, diagnostic: Option<String> }`:
   Engine suffered excessive repeated failures (6 crashes within 15 minutes). Automatic restarts are disabled. Audio endpoint outputs pure digital silence. Preserves all diagnostic crash details. Requires an explicit user reset via IPC (`RestartGeneration`).

---

## 3. Normative Exponential Backoff & Crash Thresholds

To prevent crash loops from exhausting CPU resources or destabilizing host audio daemons (PipeWire, PulseAudio, CoreAudio, WASAPI), the supervisor enforces strict backoff:

```rust
pub const BACKOFF_SECONDS: [u64; 5] = [1, 2, 4, 8, 16];
pub const MAX_CRASHES_PER_15_MINUTES: usize = 5;
```

### Retry Sequence:
- Crash 1: 1-second delay
- Crash 2: 2-second delay
- Crash 3: 4-second delay
- Crash 4: 8-second delay
- Crash 5: 16-second delay
- Crash 6 (within 15 minutes): Transition to `TerminalSafeState`

### Sliding Window Pruning
Timestamps older than 15 minutes are automatically purged from the tracker. If a crash occurs 16 minutes after an earlier crash, the earlier crash does not count against the 5-crash threshold.

---

## 4. Digital Silence Enforcement

When the supervisor is in `TerminalSafeState`, `EngineUnavailable`, or when operating in `DenoiseMode::Mute`, the audio output buffer is unconditionally zeroed out:

```rust
pub fn process_frame_or_silence(&self, frame: &mut [f32]) {
    if !self.is_running() || self.is_terminal() || self.mode == DenoiseMode::Mute {
        frame.fill(0.0);
    }
}
```

This ensures zero clicks, pops, bursts, or static reach the user's communications endpoints during fault conditions.

---

## 5. Platform Service Installation & Packaging

The binary supports native installation into user-level service supervisors:

### Command Line Interface
- `realtime-noise-service --run`: Launches the supervisor and IPC listener.
- `realtime-noise-service --install-user-service`: Installs the autostart service for the current user.
- `realtime-noise-service --uninstall-user-service`: Removes the service registration.

### 5.1 Linux (`systemd --user`)
- Unit file: `~/.config/systemd/user/realtime-noise.service`
- Management:
  ```bash
  systemctl --user daemon-reload
  systemctl --user enable --now realtime-noise.service
  systemctl --user status realtime-noise.service
  ```

### 5.2 macOS (`LaunchAgent`)
- Plist file: `~/Library/LaunchAgents/com.clearcore.realtime-noise.plist`
- Management:
  ```bash
  launchctl load ~/Library/LaunchAgents/com.clearcore.realtime-noise.plist
  launchctl unload ~/Library/LaunchAgents/com.clearcore.realtime-noise.plist
  ```

### 5.3 Windows (`schtasks`)
- Registered task: `RealtimeNoiseService`
- Trigger: At user log on (`/SC ONLOGON`)
- Run level: Limited user session (`/RL LIMITED`)
- Management scripts:
  - `platform/windows/service/register-user-service.ps1`
  - `platform/windows/service/unregister-user-service.ps1`
