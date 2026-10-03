# ClearCore - AI Coding Assistant & LLM Agent Guide

> **Target Audience:** AI Coding Assistants, LLMs, and Autonomous Software Agents (Claude, ChatGPT, Gemini, Antigravity, Cursor, Copilot Workspace, Windsurf, Devin, etc.).  
> **Curl-Friendly Direct URL:**  
> ```bash
> curl -sSL https://raw.githubusercontent.com/joaorura/clearcore/master/LLM.md
> ```

---

## Agent System Prompt & Identity Context

When assisting a developer with ClearCore, act as a **Principal Audio Systems & Realtime Rust Engineer**. You understand digital signal processing (DSP), neural audio inference, OS kernel/driver audio subsystems (PipeWire, WaveRT, CoreAudio HAL), and strict memory-safety contracts.

---

## 1. Codebase Architecture & Repository Map

ClearCore is a source-available (PolyForm Noncommercial 1.0.0), ultra-low-latency realtime noise-suppression virtual microphone powered by **DeepFilterNet3 ONNX** and an asynchronous Rust supervisor daemon.

```
hippocamp/
├── Cargo.toml                  # Workspace manifest with strict `#![forbid(unsafe_code)]` lints
├── crates/
│   ├── contracts/              # Shared data contracts (AudioFrame [f32; 480], 48 kHz, statuses)
│   ├── engine/                 # Audio processing engine (Tract ONNX DFN3 + fallback DSP gate)
│   ├── filter-capi/            # C ABI dynamic library (libclearcore_filter.so / .dll / .dylib)
│   ├── format-adapter/         # Bounded lock-free ring buffer & sample format converters
│   ├── ipc/                    # Control-plane protocol (`realtime-noise.v1` JSON-over-socket/pipe)
│   ├── model/                  # DeepFilterNet3 model asset loader & Ed25519 signature validator
│   ├── service/                # Background daemon executable (`realtime-noise-service`)
│   ├── supervisor/             # State machine supervisor, crash recovery, & health monitoring
│   ├── app-tauri/              # Desktop companion app (Electron + Vite React UI + Rust tray)
│   ├── diagnostics/            # Structured telemetry, crash logging, and diagnostics schema
│   ├── qualification/          # Deterministic offline quality gates and test benchmarks
│   ├── accelerators/           # Hardware acceleration backends (AVX2, TensorRT, OpenVINO, CoreML)
│   ├── tools/                  # Developer utilities and offline qualification harnesses
│   └── workspace-policy/       # Cargo workspace lint and safety assertion enforcement
├── platform/
│   ├── linux/
│   │   ├── helper/             # Native C PipeWire virtual microphone bridge (`pipewire_helper`)
│   │   └── host/               # Rust PipeWire host backend (`realtime-noise-linux-host`)
│   ├── windows/
│   │   ├── driver/             # WaveRT PortCls SysVAD virtual audio streaming driver (`RealtimeNoise.inf`)
│   │   └── host/               # WASAPI exclusive capture and WaveRT driver controller
│   └── macos/
│       ├── HAL/                # CoreAudio HAL AudioServerPlugIn (`RealtimeNoiseHAL.driver`)
│       └── host/               # CoreAudio host adapter
├── scripts/                    # Automation scripts (virtual mic check, autostart, uninstaller)
├── docs/                       # Architectural specifications, IPC schemas, and test evidence
├── release/                    # Generated standalone distribution packages
├── package.sh / .bat / .ps1    # One-click full standalone packager
├── start-all.sh / stop-all.sh  # Service & tray lifecycle scripts
└── uninstall.sh / .bat / .ps1  # Universal system-clean uninstaller
```

---

## 2. Non-Negotiable Invariants & Technical Contracts

When writing code or generating scripts for ClearCore, you MUST maintain these architectural guarantees:

1. **Audio Format Contract:**
   - **Sample Rate:** `48_000 Hz` (`SAMPLE_RATE_HZ`)
   - **Channels:** `1` (`CHANNELS`, mono Float32 LE)
   - **Quantum / Hop Size:** `480` samples (`HOP_SAMPLES` = exactly `10.0 ms`)
   - Any hardware input running at 44.1 kHz, 96 kHz, or stereo must pass through `crates/format-adapter` before reaching the neural engine.

2. **Strict Realtime Thread Safety (Zero Allocations):**
   - The audio callback loop runs within high-priority OS audio threads (`pipewire_helper`, WaveRT, CoreAudio IO).
   - **Zero heap allocations** (`malloc`, `free`, `Box::new`, `Vec::push`, `String`) are permitted in the callback.
   - **Zero blocking syscalls** or unbounded lock contention (`std::sync::Mutex` is forbidden; use bounded lock-free ring buffers or atomic pointers).

3. **Fail-Closed Digital Silence Policy (Zero Audio Leakage):**
   - Under **no circumstance** may raw, unsuppressed microphone audio leak through to the output virtual microphone when the engine is in an error, restarting, unready, or uninitialized state.
   - If the neural engine fails, crashes, or underruns, the virtual output node **strictly emits digital silence (zeros)**.

4. **Memory Safety & Unsafe Policy:**
   - Workspace-level `Cargo.toml` enforces `#![forbid(unsafe_code)]`.
   - The **only** crate with `unsafe` is `crates/filter-capi` (for C ABI compatibility). Every `unsafe` block must have a documented safety comment (`// SAFETY: ...`) and pass AddressSanitizer (ASan).

5. **Local IPC Control Plane (`realtime-noise.v1`):**
   - Strictly control-plane only. **Zero audio PCM data** is sent over IPC.
   - Protocol format: Newline-delimited JSON (`\n`) over Unix domain socket (`$XDG_RUNTIME_DIR/realtime-noise.sock`) or Windows Named Pipe (`\\.\pipe\realtime-noise`).

---

## 3. Playbook for Common User Requests

### Scenario 1: "Install ClearCore on my machine"

#### Instructions for Agent:
1. Detect host operating system (`uname -s` or `$env:OS`).
2. Verify CPU AVX2 instruction support.
3. Check for existing pre-packaged release bundle; if present, run turnkey installer; otherwise, build from source.

#### Exact Commands:

**On Linux (Ubuntu / Fedora / Arch):**
```bash
# 1. Verify AVX2 support
grep -q avx2 /proc/cpuinfo && echo "AVX2 OK" || echo "Warning: AVX2 missing"

# 2. Check if release bundle exists
if [ -d "release/Clearcore-linux-x64" ]; then
    echo "Installing from existing release package..."
    ./release/Clearcore-linux-x64/install.sh
elif [ -f "Clearcore-linux-x64.tar.gz" ]; then
    tar -xzf Clearcore-linux-x64.tar.gz
    ./Clearcore-linux-x64/install.sh
else
    echo "Building and packaging from source..."
    ./package.sh
    ./release/Clearcore-linux-x64/install.sh
fi

# 3. Verify PipeWire virtual microphone
./scripts/check-virtual-mic.sh --status
```

**On Windows (PowerShell as Administrator):**
```powershell
# 1. If Clearcore-Setup.exe or bundle exists
if (Test-Path ".\release\Clearcore-win32-x64\Clearcore.exe") {
    Write-Host "Running Clearcore portable..."
    Start-Process ".\release\Clearcore-win32-x64\Clearcore.exe"
} else {
    Write-Host "Building from source..."
    .\package.ps1
    Start-Process ".\release\Clearcore-win32-x64\Clearcore.exe"
}

# 2. Verify virtual microphone
powershell .\scripts\check-virtual-mic-windows.ps1 -Status
```

**On macOS:**
```bash
if [ -d "release/Clearcore-darwin-x64/Clearcore.app" ]; then
    ./release/Clearcore-darwin-x64/install.sh
else
    ./package.sh
    ./release/Clearcore-darwin-x64/install.sh
fi
./scripts/check-virtual-mic-macos.sh --status
```

---

### Scenario 2: "Check if virtual microphone is running and working"

#### Commands to Execute:

**Universal / Cross-Platform IPC Status:**
```bash
cargo run --release -p realtime-noise-app-tauri -- --status
```
*Expected Output:*
```json
{"state":"Running","is_terminal":false,"can_restart":true,"mode":"Active","crash_count_15m":0,"total_crashes":0}
```

**Linux Audio Subsystem Status:**
```bash
# Check virtual microphone node in PipeWire graph:
./scripts/check-virtual-mic.sh --status

# Get machine-readable JSON status:
./scripts/check-virtual-mic.sh --json

# Direct PipeWire / WirePlumber verification:
wpctl status | grep -E "realtime-noise|Realtime Noise"
pactl list sources short | grep -E "realtime_noise"
```

**Windows Driver Status:**
```powershell
powershell .\scripts\check-virtual-mic-windows.ps1 -Status
```

**macOS CoreAudio HAL Status:**
```bash
./scripts/check-virtual-mic-macos.sh --status
```

---

### Scenario 3: "Change noise suppression mode (active / bypass / mute)"

ClearCore supports three operational modes via the IPC control plane:
- **`Active`**: DeepFilterNet3 neural noise suppression is applied.
- **`Bypass`**: Audio passes through clean without neural inference (original raw sound).
- **`Mute`**: Output emits digital silence.

#### Commands to Execute:

**Using the Desktop Control CLI:**
```bash
# Set mode to Active (DeepFilterNet3 Denoising):
cargo run --release -p realtime-noise-app-tauri -- --mode active

# Set mode to Bypass (Zero Filtering):
cargo run --release -p realtime-noise-app-tauri -- --mode bypass

# Set mode to Mute (Digital Silence):
cargo run --release -p realtime-noise-app-tauri -- --mode mute
```

**Direct Low-Level IPC Socket Call (Linux / macOS):**
```bash
# Send SetMode request directly over Unix socket using netcat / socat:
printf '{"version":"realtime-noise.v1","request_id":"req-01","command":{"SetMode":"Active"},"payload":{}}\n' | nc -U "${XDG_RUNTIME_DIR:-/tmp}/realtime-noise.sock"
```

---

### Scenario 4: "Diagnose audio or noise leakage"

When a user suspects noise leakage, latency spikes, or audio dropout:

#### Diagnostic Checklist & Commands:
1. **Run the Offline Qualification Gate:**
   ```bash
   ./scripts/check-offline.sh
   ```
   Ensures deterministic model integrity, Ed25519 signature validity, and zero-allocation assertions.

2. **Inspect Background Service Logs:**
   ```bash
   # On Linux:
   tail -n 100 /tmp/realtime-noise-service.log
   tail -n 100 /tmp/realtime-noise-helper.log
   ```

3. **Check Latency and Noise Floor Metrics:**
   - **Neural Inference Latency:** Verified at **0.276 ms** per 10 ms frame (Tract AVX2).
   - **Noise Reduction Factor:** **-32.8 dB** ambient reduction.
   - **Noise Floor:** **-70.3 dBFS** digital silence floor.

4. **Verify Quantum Synchronization:**
   If audio crackles or drops out, verify PipeWire quantum is locked to 480 @ 48 kHz:
   ```bash
   pw-metadata -n settings 0 clock.force-rate 48000
   pw-metadata -n settings 0 clock.force-quantum 480
   ```

5. **Restart Pipeline Generation (Self-Healing):**
   ```bash
   cargo run --release -p realtime-noise-app-tauri -- --restart
   ```

---

### Scenario 5: "Uninstall ClearCore completely"

ClearCore provides isolated, cleanly scoped uninstallers that leave no residual audio state:

#### Commands to Execute:

**Linux / macOS:**
```bash
# Run universal uninstaller:
./uninstall.sh

# If installed system-wide with sudo:
sudo ./uninstall.sh
```

**Windows:**
```powershell
# From Command Prompt:
uninstall.bat

# Or from PowerShell (Administrator recommended for WaveRT driver unregistration):
powershell -NoProfile -ExecutionPolicy Bypass -File .\uninstall.ps1
```

**Post-Uninstallation Verification:**
```bash
# Verify no processes remain:
pgrep -l "clearcore|pipewire_helper|realtime-noise" || echo "All processes terminated cleanly"

# Verify PipeWire node is destroyed:
pw-cli list-objects Node | grep "realtime-noise-source" || echo "Node removed cleanly"
```

---

## 4. Key Code Locations for Quick Navigation

- **Audio Contracts & Types:** [`crates/contracts/src/audio.rs`](file:///crates/contracts/src/audio.rs)
- **Neural Engine & Tract ONNX:** [`crates/engine/src/lib.rs`](file:///crates/engine/src/lib.rs)
- **IPC Protocol Definition:** [`crates/ipc/src/protocol.rs`](file:///crates/ipc/src/protocol.rs) & [`docs/ipc-v1.md`](file:///docs/ipc-v1.md)
- **Supervisor State Machine:** [`crates/supervisor/src/`](file:///crates/supervisor/src/)
- **Linux PipeWire Native C Helper:** [`platform/linux/helper/src/pipewire_helper.c`](file:///platform/linux/helper/src/pipewire_helper.c)
- **Windows WaveRT Driver Spec:** [`platform/windows/driver/RealtimeNoise.inf`](file:///platform/windows/driver/RealtimeNoise.inf)
- **macOS CoreAudio HAL Driver:** [`platform/macos/HAL/`](file:///platform/macos/HAL/)
- **Full Install Runbook:** [`INSTALL.md`](file:///INSTALL.md)
