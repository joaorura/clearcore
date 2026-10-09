# ClearCore Stage 2 Technical Design: Connecting pDFNet3 Pro v2 and Voice Profile Conditioning to the Native Virtual Microphone

**Date:** 2026-10-09  
**Status:** Approved design specification; implementation ready  
**Repositories:** `clearcore` and `clearcore-train`

## 1. Goals and Background

ClearCore delivers real-time voice enhancement and noise suppression through a native system virtual microphone. In Stage 1, we implemented the Rust control path (`realtime-noise-service`), which handles user enrollment takes, voice activity detection (VAD), audio leveling, ECAPA-TDNN speaker embedding extraction, profile compilation, and secure disk persistence. However, on Linux the shipping virtual microphone (`realtime-noise-virtual-mic`) does not run inside the Rust service daemon. Instead, it runs as a dedicated native C process (`pipewire_helper`), dynamically loading `libclearcore_filter.so` via `crates/filter-capi`.

Currently, `crates/filter-capi` only verifies and instantiates the base DeepFilterNet3 model asset from `ApprovedAssetManifest`. It provides zero C-ABI entry points for voice profiles. As a result, the live PipeWire audio stream processed by the virtual microphone remains unconditioned. It removes background noise generically, but cannot isolate the enrolled speaker from secondary voices.

Stage 2 closes this gap. This specification details how to connect the personalized pDFNet3 Pro v2 model and `VoiceProfile` conditioning into the native virtual microphone pipeline through `filter-capi` and `pipewire_helper`.

### Model Asset Provenance

The neural model asset for Stage 2 is the pDFNet3 Pro v2 model produced by `clearcore-train`:

- **Run Directory:** `runs/m3_deploy_pro_v2_20261009`
- **Asset Archive Path:** `/home/joaorura/orca/projects/clearcore-train/runs/m3_deploy_pro_v2_20261009/pdfnet3-release-asset-v1.tar.gz`
- **Expected SHA-256:** `a3db32ae85a1c9dc81d97a548cc1d5c0c411186ff95fab8591d91d553453bdb2`
- **Archive Members:** `enc.onnx`, `erb_dec.onnx`, `df_dec.onnx`, `config.ini`

pDFNet3 Pro v2 extends the DeepFilterNet3 backbone with Feature-wise Linear Modulation (FiLM) layers in both the ERB encoder (`enc.onnx`) and the DeepFilter decoder (`df_dec.onnx`). When supplied with speaker-specific FiLM vectors (`gamma`, `beta`), the model modulates its internal recurrent representations to isolate the enrolled speaker's vocal characteristics while attenuating background voices and environmental noise. When supplied with identity FiLM vectors (`gamma = 1.0, beta = 0.0`), the model operates as a neutral, high-quality denoiser without speaker rejection.

## 2. Architectural Separation: Stage 1 vs Stage 2

ClearCore strictly separates its control plane from its real-time audio data plane.

```text
+-----------------------------------------------------------------------------+
| STAGE 1: Control Plane (Rust Service Daemon: realtime-noise-service)        |
|                                                                             |
|  - Client IPC Server (realtime-noise.v1 domain socket)                      |
|  - Voice Enrollment Pipeline (Intake, VAD, Leveling, ECAPA-TDNN ONNX)       |
|  - Profile Builder (FiLM projection, Spectral EQ calibration)               |
|  - Profile Store: ~/.local/share/clearcore/profiles/active_profile.json     |
|  - File permissions enforcement (dir: 0700, file: 0600)                     |
|  - ServiceDaemon engine supervisor and status telemetry                     |
+-----------------------------------------------------------------------------+
                                      |
                     Filesystem and Shared Memory State
                                      |
                                      v
+-----------------------------------------------------------------------------+
| STAGE 2: Real-Time Audio Data Plane (Native Virtual Mic: pipewire_helper)   |
|                                                                             |
|  - PipeWire SPA audio capture and playback callbacks (on_capture_process)   |
|  - Lock-free shared memory tracking ($XDG_RUNTIME_DIR/clearcore_state)      |
|  - Dynamic loading of libclearcore_filter.so via dlopen/dlsym               |
|  - crates/filter-capi: C-ABI wrappers around StudioBackend<TractBackend>    |
|  - Dedicated Tract background worker thread for inference and conditioning  |
|  - Zero-allocation audio thread invariant                                   |
|  - Fail-closed digital silence fallback policy                              |
+-----------------------------------------------------------------------------+
```

### Stage 1: Rust Service Daemon

The Rust service daemon handles all non-real-time operations:

1. It receives user audio takes over IPC, runs VAD trimming, validates signal quality, and computes speaker embeddings.
2. It compiles the resulting FiLM parameters and spectral EQ gains into a validated `VoiceProfile` struct.
3. It writes the active profile atomically to `<data dir>/clearcore/profiles/active_profile.json` using strict POSIX file permissions (`0600` file, `0700` parent directory).
4. It exposes service health and enrollment status over IPC to the Electron desktop application.

Because the service runs on standard worker threads, it can allocate heap memory, execute disk I/O, parse JSON, and run heavy neural embeddings without latency deadlines.

### Stage 2: Native Virtual Microphone

The native virtual microphone runs as an independent OS-level audio process:

1. `pipewire_helper` binds directly to the PipeWire graph via `pw_stream` and receives audio frames in the real-time capture callback (`on_capture_process`).
2. It processes audio strictly in 10 ms hops (480 samples at 48 kHz mono float32) within a sub-millisecond execution budget.
3. It forbids heap allocations (`malloc`, `free`), blocking syscalls, and mutex locks inside the audio callback.
4. It synchronizes operating modes (`Active`, `Bypass`, `Mute`) and studio finishing presets (`Off`, `Natural`, `Podcast`, `Broadcast`) through the 16-byte shared memory file `clearcore_state`.

Stage 2 must not perform file reading or JSON deserialization inside `on_capture_process`. Instead, profile loading and conditioning occur outside the real-time audio callback, passing updates to Tract's internal worker thread via bounded channels.

## 3. Model Loading and Conditioning Policy

### 3.1 Model Loading Hierarchy

When initializing the neural filter in `crates/filter-capi`, the library strictly requires the pDFNet3 Pro v2 model without fallback:

```text
                     clearcore_filter_create(repo_root)
                                     |
                                     v
             +-----------------------------------------------+
             | Is pDFNet3 Pro v2 Archive Available & Valid?  |
             +-----------------------------------------------+
                     /                               \
               YES  /                                 \  NO
                   v                                   v
+------------------------------------+   +------------------------------------+
| Load pDFNet3 Pro v2 Archive        |   | Fail-Closed: Return NULL           |
| - Verify SHA-256 (a3db32ae...)     |   | - No fallback to base model        |
| - Validate archive members allowlist|  | - Virtual mic refuses creation     |
| - TractBackend detects FiLM inputs |   +------------------------------------+
| - film_supported = true            |
+------------------------------------+
```

1. **pDFNet3 Pro v2 Archive (Personalized Isolation - Mandatory)**
   The loader checks for the pDFNet3 Pro v2 asset archive:
   - Environment variables: `CLEARCORE_DEV_PDFNET3_ASSET` and `CLEARCORE_DEV_PDFNET3_SHA256`.
   - Default deployment path: `/home/joaorura/orca/projects/clearcore-train/runs/m3_deploy_pro_v2_20261009/pdfnet3-release-asset-v1.tar.gz`.
   - Expected SHA-256 digest: `a3db32ae85a1c9dc81d97a548cc1d5c0c411186ff95fab8591d91d553453bdb2`.

   The loader computes the SHA-256 over the entire archive. If the digest matches, it inspects the tar archive to verify that every member belongs to the strict allowlist (`enc.onnx`, `erb_dec.onnx`, `df_dec.onnx`, `config.ini`) and contains no symlinks or executable bits. If verification passes, it initializes `TractBackend` from the archive. Tract detects the 4-input graphs for encoder and decoder, setting `film_supported = true`.

2. **Strict Fail-Closed Policy (No Fallback)**
   If the pDFNet3 Pro v2 archive is missing, incomplete, or fails hash verification, filter creation fails closed and returns `NULL`. Fallback to the legacy unconditioned base model is intentionally omitted so the system guarantees execution of the Pro model.

### 3.2 FiLM Conditioning State Machine

Once loaded, `TractBackend` enforces deterministic conditioning behavior:

| Model Loaded | Profile Requested | Applied Conditioning | Resulting Audio Behavior |
| :--- | :--- | :--- | :--- |
| pDFNet3 Pro v2 | `None` (or cleared) | Identity FiLM (`gamma=1.0, beta=0.0`), flat EQ | Neutral high-grade denoiser. All speech preserved. |
| pDFNet3 Pro v2 | `Some(profile)` | Speaker FiLM (`gamma_enc, beta_enc, gamma_df, beta_df`), spectral EQ | Personalized isolation. Target voice enhanced, others rejected. |
| Base DFNet3 | `None` (or cleared) | No FiLM modification | Standard base denoiser. Passthrough conditioning. |
| Base DFNet3 | `Some(profile)` (non-identity) | Rejected with typed error | Fails closed. Previous base denoiser state preserved. |

When no profile is active, pDFNet3 Pro v2 does not mute or reject the speaker. It applies identity FiLM scaling, ensuring that the model functions as an unconditioned speech denoiser.

## 4. C-ABI Interface Additions in `crates/filter-capi`

To allow native audio hosts such as `pipewire_helper` to manage voice profiles, `crates/filter-capi/src/lib.rs` exports new C-ABI functions.

### 4.1 Internal Handle Structure

`ClearcoreFilter` wraps `StudioBackend<TractBackend>` alongside profile tracking fields:

```rust
pub struct ClearcoreFilter {
    backend: StudioBackend<TractBackend>,
    control: Arc<StudioControl>,
    supports_film: bool,
    active_profile_id: Option<String>,
}
```

### 4.2 C-ABI Functions

#### 1. `clearcore_filter_set_voice_profile`

Activates a voice profile directly from a JSON string payload.

```rust
/// Activate a voice profile from a JSON string.
///
/// # Arguments
/// - `filter`: Valid pointer returned by `clearcore_filter_create`.
/// - `profile_json`: Null-terminated UTF-8 C string containing serialized `VoiceProfile` JSON.
///
/// # Returns
/// - `0`: Success. Profile validated and applied to inference backend.
/// - `-1`: Invalid arguments (`filter` or `profile_json` is NULL, or non-UTF-8).
/// - `-2`: JSON deserialization failed or profile integrity check failed.
/// - `-3`: The loaded model does not support speaker conditioning (base model).
/// - `-4`: Backend conditioning execution error.
///
/// # Safety
/// Must be called outside the realtime audio processing callback.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clearcore_filter_set_voice_profile(
    filter: *mut ClearcoreFilter,
    profile_json: *const c_char,
) -> i32;
```

Implementation details:
1. Validates that `filter` and `profile_json` are non-null.
2. Converts `profile_json` to a Rust string slice.
3. Deserializes the JSON into a `VoiceProfile` struct using `serde_json`.
4. Executes `profile.verify_integrity()`, ensuring the embedded HMAC and SHA-256 match the vector contents.
5. Invokes `filter.backend.set_voice_profile(Some(&profile))`.
6. On success, caches `profile.id` in `filter.active_profile_id` and returns `0`.

#### 2. `clearcore_filter_clear_voice_profile`

Clears the active profile and restores neutral identity conditioning.

```rust
/// Clear the active voice profile and revert to neutral identity conditioning.
///
/// # Arguments
/// - `filter`: Valid pointer returned by `clearcore_filter_create`.
///
/// # Returns
/// - `0`: Success. Backend restored to neutral identity conditioning.
/// - `-1`: `filter` is NULL.
/// - `-2`: Backend conditioning execution error.
///
/// # Safety
/// Must be called outside the realtime audio processing callback.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clearcore_filter_clear_voice_profile(
    filter: *mut ClearcoreFilter,
) -> i32;
```

Implementation details:
1. Calls `filter.backend.set_voice_profile(None)`.
2. For pDFNet3 Pro v2, this sends identity FiLM vectors (`gamma = 1.0, beta = 0.0`) and resets spectral EQ to flat.
3. Sets `filter.active_profile_id = None` and returns `0`.

#### 3. `clearcore_filter_reload_active_profile`

Reads the persisted active profile from the secure profile store on disk and applies it.

```rust
/// Reload the active voice profile from standard disk storage.
///
/// Checks ~/.local/share/clearcore/profiles/active_profile.json (or OS equivalent).
/// If present and valid, loads and applies the profile.
/// If absent, clears the active profile and returns to neutral conditioning.
///
/// # Arguments
/// - `filter`: Valid pointer returned by `clearcore_filter_create`.
/// - `repo_root_path`: Optional repository root C string (NULL defaults to current directory).
///
/// # Returns
/// - `0`: No profile file found on disk; neutral conditioning applied successfully.
/// - `1`: Active profile loaded from disk and applied successfully.
/// - `-1`: `filter` is NULL.
/// - `-2`: Profile file exists but failed verification (bad permissions, corrupted JSON, bad hash).
/// - `-3`: The loaded model does not support speaker conditioning.
/// - `-4`: Backend conditioning execution error.
///
/// # Safety
/// Must be called outside the realtime audio processing callback.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clearcore_filter_reload_active_profile(
    filter: *mut ClearcoreFilter,
    repo_root_path: *const c_char,
) -> i32;
```

Implementation details:
1. Resolves `ProfileStore::default_dir()`.
2. Inspects `<profile_dir>/active_profile.json`.
3. Verifies file security: file must be a regular file owned by the current user, without symlinks, and with mode `0600`.
4. If the file does not exist, it calls `clearcore_filter_clear_voice_profile` and returns `0`.
5. If the file exists, it reads and validates the profile with `ProfileStore::load_active()`.
6. Passes the validated profile to `filter.backend.set_voice_profile(Some(&profile))`.
7. On success, updates `filter.active_profile_id` and returns `1`.

#### 4. Diagnostic Queries

```rust
/// Returns 1 if the loaded neural model supports speaker conditioning (pDFNet3 Pro v2),
/// or 0 if running the unconditioned base model. Returns -1 if filter is NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clearcore_filter_supports_voice_profile(
    filter: *const ClearcoreFilter,
) -> i32;

/// Returns 1 if a personalized voice profile is currently active,
/// or 0 if running neutral/unconditioned. Returns -1 if filter is NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clearcore_filter_is_voice_profile_active(
    filter: *const ClearcoreFilter,
) -> i32;
```

## 5. PipeWire Helper Integration

The native Linux virtual microphone is implemented in `platform/linux/helper/src/pipewire_helper.c`. It loads `libclearcore_filter.so` at runtime using `dlopen`.

### 5.1 C-ABI Function Pointer Signatures

In `platform/linux/helper/src/pipewire_helper.h`, add the function pointer types:

```c
typedef int (*clearcore_set_voice_profile_fn_t)(void *filter, const char *profile_json);
typedef int (*clearcore_clear_voice_profile_fn_t)(void *filter);
typedef int (*clearcore_reload_active_profile_fn_t)(void *filter, const char *repo_root);
typedef int (*clearcore_supports_voice_profile_fn_t)(const void *filter);
typedef int (*clearcore_is_voice_profile_active_fn_t)(const void *filter);
```

Extend `pipewire_helper_context_t`:

```c
/* Voice profile control function pointers */
clearcore_set_voice_profile_fn_t neural_set_voice_profile_fn;
clearcore_clear_voice_profile_fn_t neural_clear_voice_profile_fn;
clearcore_reload_active_profile_fn_t neural_reload_active_profile_fn;
clearcore_supports_voice_profile_fn_t neural_supports_voice_profile_fn;
clearcore_is_voice_profile_active_fn_t neural_is_voice_profile_active_fn;

/* Profile reload synchronization */
_Atomic uint32_t applied_generation;
struct pw_source *profile_timer_source;
```

### 5.2 Dynamic Symbol Resolution and Startup Loading

During helper initialization (`neural_filter_init` in `pipewire_helper.c`):

1. Resolve the existing symbols (`clearcore_filter_create`, `clearcore_filter_process`, `clearcore_filter_set_preset`, `clearcore_filter_free`).
2. Resolve the new voice profile symbols (`clearcore_filter_set_voice_profile`, `clearcore_filter_clear_voice_profile`, `clearcore_filter_reload_active_profile`, `clearcore_filter_supports_voice_profile`, `clearcore_filter_is_voice_profile_active`). If missing (e.g. against an older library build), set pointers to NULL without aborting.
3. Call `clearcore_filter_create` across repo candidates.
4. Immediately after creating the filter, invoke `clearcore_filter_reload_active_profile(filter, repo_path)`.
5. Run the 5-frame neural warmup. The warmup pass now warms up either the personalized path or the neutral identity path.
6. Record whether speaker conditioning is supported by checking `neural_supports_voice_profile_fn`.

### 5.3 Runtime Profile Synchronization and Reload Triggers

When a user records new voice takes or updates their profile in the desktop app, the updated profile is saved to disk by Stage 1. The helper must reload the profile promptly without disturbing real-time audio.

We use a dual reload trigger:

```text
                  USER UPDATES VOICE PROFILE IN DESKTOP UI
                                     |
                                     v
                  Stage 1 saves active_profile.json (0600)
                                     |
               +---------------------+---------------------+
               |                                           |
               v                                           v
    Atomically updates generation               Sends SIGUSR1 signal
    in $XDG_RUNTIME_DIR/clearcore_state         to pipewire_helper PID
               |                                           |
               v                                           v
    PipeWire main loop timer (250 ms)           PipeWire main loop signal handler
    detects generation mismatch                            |
               |                                           |
               +---------------------+---------------------+
                                     |
                                     v
                  Main loop thread calls C-ABI:
                  clearcore_filter_reload_active_profile()
                                     |
                                     v
                  Tract backend worker updates FiLM
                  between 10 ms frame hops without resetting GRUs
```

1. **Shared State Generation Polling (Automatic, Non-Blocking)**
   The shared memory file `clearcore_state` has an existing 4-byte atomic field at offset 8: `_Atomic uint32_t generation`. Previously unused by the helper, this field now acts as the profile configuration counter.
   - When the Electron app or Rust service daemon writes or clears `active_profile.json`, it increments `generation` in `clearcore_state`.
   - In `pipewire_helper`, a lightweight timer attached to PipeWire's `pw_main_loop` fires every 250 ms on the main thread (outside the audio thread).
   - The timer compares `generation` with `applied_generation`. If they differ, the main loop calls `clearcore_filter_reload_active_profile`, logs the transition, and updates `applied_generation`.

2. **POSIX Signal Trigger (`SIGUSR1`)**
   For instantaneous updates without waiting for the 250 ms poll:
   - `pipewire_helper` registers a `SIGUSR1` handler via `pw_loop_add_signal`.
   - On receiving `SIGUSR1`, the signal callback on the main loop triggers `clearcore_filter_reload_active_profile` immediately.

### 5.4 Audio Callback Execution Discipline

In `on_capture_process` (the real-time callback):

```c
/* Active mode processing */
if (ctx->neural_filter && ctx->neural_process_fn) {
    apply_studio_preset(ctx);
    int rc = ctx->neural_process_fn(ctx->neural_filter, raw_frame, processed_frame);
    if (rc != 0) {
        /* Fallback to DSP suppressor if neural inference fails */
        noise_suppressor_process(&ctx->suppressor, raw_frame, processed_frame, HOP_SAMPLES);
    }
}
```

The audio callback remains clean, fast, and simple. It performs zero disk operations, zero allocations, and zero string operations. It executes forward neural inference, which automatically uses the updated FiLM vectors maintained by Tract's worker.

## 6. Real-Time Audio Safety and Invariants

Real-time audio processing imposes strict constraints. Any violation leads to audible glitches, xruns, or latency spikes.

### 6.1 Zero Heap Allocations in Audio Path

The PipeWire capture callback `on_capture_process` must perform zero heap allocations (`malloc`, `calloc`, `realloc`, `free`).

- **Verification:** ClearCore's automated test suite runs `test_neural_filter` with linker wrapping (`-Wl,--wrap=malloc`, `-Wl,--wrap=calloc`, `-Wl,--wrap=realloc`, `-Wl,--wrap=free`). Any allocation during processing causes immediate test abort.
- **Fixed Memory Footprint:** All frame buffers (`raw_frame`, `processed_frame`, accumulator buffers) are statically sized at 480 floats (`HOP_SAMPLES`).
- **Conditioning Isolation:** All parsing, memory allocations, and vector clones for voice profiles occur strictly inside `clearcore_filter_set_voice_profile` on the control thread, never inside `clearcore_filter_process`.

### 6.2 State Continuity and Glitch-Free Transitions

When voice profile conditioning changes (for example, switching from neutral to active profile, or swapping between speakers), the audio stream must remain continuous:

1. **GRU Hidden State Preservation:**
   DeepFilterNet3 uses Gated Recurrent Units (`emb_gru` in encoder, `df_gru` in decoder) that maintain running temporal memory across audio frames. Resetting GRU hidden states produces an audible pop and degrades initial speech syllables.
   In `TractBackend`, processing `WorkerCommand::Condition` updates only the `FilmVectors` scaling factors and spectral EQ bin factors. It preserves all recurrent state tensors (`h_enc`, `h_df`) and delay lines.
2. **Hop-Synchronized Transition:**
   Conditioning updates apply strictly between 10 ms frame boundaries. A single 480-sample frame never sees partial or mismatched FiLM parameters.
3. **No Retraining or Warmup Dropouts:**
   Because recurrent states are kept intact, switching profiles requires zero warm-up frames. The next audio hop immediately uses the new conditioning vectors.

### 6.3 Fail-Closed Safety Hierarchy

If any failure occurs during real-time capture:

1. If neural inference returns a non-zero error code, the pipeline falls back to the deterministic DSP noise gate and expander (`noise_suppressor_process`).
2. If DSP processing encounters an error or buffer underflow, the output buffer is filled with digital silence (`format_converter_zero_silence`).
3. Raw unprocessed microphone audio is never passed through when in Active mode, eliminating the risk of ambient room leakage.

## 7. Desktop UI Updates in `crates/app-tauri`

The desktop companion app must accurately reflect whether voice isolation is active in the live virtual microphone.

### 7.1 Status Model Extensions

Extend the IPC status schema and TypeScript types in `crates/app-tauri/src/types.ts`:

```typescript
export interface VoiceProfileStatus {
  is_enrolled: boolean;
  active_samples_count: number;
  embedding_dim: number;
  neural_eq_calibrated: boolean;
  gain_boost_db?: number;
  is_voice_profile_active?: boolean;
  stored_voice_profile_id?: string;
  voice_profile_error?: string;
  voice_profile_selected?: string;
  active_voice_profile_id?: string;
  has_voice_profile?: boolean;
  voice_profile_supported?: boolean;
  dev_base_model?: 'pdfnet3-dev' | 'base';
  dev_base_model_error?: string | null;
  voice_isolation_enabled?: boolean;
  
  /* Stage 2 Additions */
  virtual_mic_voice_profile_active?: boolean;
  virtual_mic_model_type?: 'pdfnet3-pro-v2' | 'dfnet3-base';
  virtual_mic_helper_connected?: boolean;
}
```

### 7.2 Truthful UI State Machine

The UI in `crates/app-tauri/src/VoiceProfileCard.tsx` and `crates/app-tauri/src/voice/panels/profileStatusLabel.ts` presents distinct states:

```text
+----------------------------+------------------------------------------------------+
| State Key                  | Badge & Visual Presentation                          |
+----------------------------+------------------------------------------------------+
| Active (Virtual Mic)       | Green badge: "Isolating Voice (Virtual Mic)"         |
|                            | Confirmed active in live PipeWire virtual microphone |
+----------------------------+------------------------------------------------------+
| Active (Daemon Only)       | Amber badge: "Profile Ready (Virtual Mic Inactive)"  |
|                            | Profile ready in daemon, but helper running base     |
+----------------------------+------------------------------------------------------+
| Neutral Denoiser           | Blue badge: "Standard Denoising (No Profile)"        |
|                            | pDFNet3 Pro v2 running with identity FiLM            |
+----------------------------+------------------------------------------------------+
| Base Denoiser              | Grey badge: "Generic Denoiser (Base Model)"          |
|                            | Base DFNet3 running without voice profile support    |
+----------------------------+------------------------------------------------------+
```

The UI never displays "Voice Isolation Active" if only the daemon has loaded the profile while the virtual microphone is running base DFNet3.

### 7.3 Desktop State Synchronization

When the user activates or clears a profile in the desktop UI:

1. The React frontend sends IPC commands (`SetVoiceProfile` or `ClearVoiceProfile`) to the service daemon.
2. The service daemon writes `<data dir>/clearcore/profiles/active_profile.json`.
3. The Electron main process calls `updateStateFile` in `clearcore-state.cjs`, incrementing the `generation` field at offset 8.
4. If on Linux and `pipewire_helper` is running, Electron optionally sends `SIGUSR1` to the helper process.
5. `pipewire_helper` reloads the active profile and updates its local status.
6. The UI queries status and confirms that `virtual_mic_voice_profile_active` is true.

## 8. Testing Strategy and Quality Gates

Implementation follows a strict test-driven development order:

```text
[Step 1: C-ABI Unit Tests]  -->  [Step 2: Memory & Safety Tests]  -->  [Step 3: Helper Integration]
       crates/filter-capi              Linker __wrap_malloc               platform/linux/helper
                                                 |
                                                 v
[Step 5: End-to-End QA]     <--  [Step 4: Desktop UI Verification]
  Real PipeWire Audio Stream         crates/app-tauri Vitest & E2E
```

### 8.1 Rust C-ABI Unit Tests (`crates/filter-capi/tests`)

1. `test_filter_create_prefers_pdfnet3_pro`:
   Verifies that when `CLEARCORE_DEV_PDFNET3_ASSET` is set with the correct SHA-256, the filter initializes pDFNet3 Pro v2 and `clearcore_filter_supports_voice_profile` returns 1.
2. `test_filter_set_voice_profile_valid`:
   Creates a filter with pDFNet3 Pro v2, serializes a valid `VoiceProfile`, calls `clearcore_filter_set_voice_profile`, and verifies that it returns 0. Confirms that `clearcore_filter_is_voice_profile_active` returns 1.
3. `test_filter_set_voice_profile_integrity_failure`:
   Alters a single float in the profile's FiLM vectors without updating the HMAC. Verifies that the C-ABI returns `-2` and leaves the existing state intact.
4. `test_filter_clear_voice_profile`:
   Sets an active profile, then calls `clearcore_filter_clear_voice_profile`. Verifies that the C-ABI returns 0 and resets active status to 0.
5. `test_filter_reload_active_profile`:
   Writes a temporary `active_profile.json` with permissions `0600`. Calls `clearcore_filter_reload_active_profile`. Verifies that it returns 1. Deletes the file, reloads again, and verifies that it returns 0 (cleared to neutral).

### 8.2 Real-Time Audio Safety Tests (`platform/linux/helper/tests`)

1. `test_neural_filter_zero_alloc`:
   Processes 1,000 synthetic audio frames through `clearcore_filter_process` while toggling profiles on a separate control thread. Verifies that `__wrap_malloc` counts zero allocations on the processing thread.
2. `test_audio_continuity_across_profile_switch`:
   Feeds a continuous 120 Hz harmonic voiced speech signal into the filter. Toggles between neutral FiLM and an active voice profile. Measures sample-to-sample difference across frame boundaries to verify the absence of clicks, DC offsets, or phase discontinuities.

### 8.3 Helper Integration Tests

1. `test_pipewire_helper_startup_profile`:
   Launches `pipewire_helper` in a test environment with an active profile file present on disk. Verifies via stderr diagnostic logs that the profile was detected and applied at startup.
2. `test_pipewire_helper_generation_reload`:
   Runs `pipewire_helper`. Updates `generation` in `clearcore_state`. Verifies that the helper reloads the profile within 300 ms without audio drops.

### 8.4 Offline Gate Compliance

Run `./scripts/check-offline.sh` to ensure:
- Zero unpinned external network access.
- All dependencies verified in offline vendor cache.
- `#![forbid(unsafe_code)]` compliance maintained across all core crates.

## 9. Delivery Boundaries and Acceptance Criteria

### 9.1 Stage Boundaries

- **Stage 1 (Completed):** Rust control plane. Enrollment, intake, ECAPA-TDNN extraction, profile storage, IPC protocol.
- **Stage 2 (This Specification):** Data plane integration. pDFNet3 Pro v2 loading in `filter-capi`, C-ABI profile control functions, `pipewire_helper` startup and runtime reload hooks, zero-allocation verification, and desktop UI status reporting.

### 9.2 Acceptance Criteria

1. **Deterministic Model Selection:** `filter-capi` loads pDFNet3 Pro v2 from `/home/joaorura/orca/projects/clearcore-train/runs/m3_deploy_pro_v2_20261009/pdfnet3-release-asset-v1.tar.gz` when its SHA-256 matches `a3db32ae85a1c9dc81d97a548cc1d5c0c411186ff95fab8591d91d553453bdb2`. If missing or invalid, it falls back to the base model without panicking.
2. **Neutral Identity FiLM Default:** When pDFNet3 Pro v2 is loaded without an active profile, it applies identity FiLM scaling (`gamma=1.0, beta=0.0`), operating as a clean denoiser without speech suppression.
3. **C-ABI Completeness:** `clearcore_filter_set_voice_profile`, `clearcore_filter_clear_voice_profile`, `clearcore_filter_reload_active_profile`, and query functions are exported with well-defined error codes.
4. **PipeWire Helper Reload:** `pipewire_helper` reloads the active profile on startup and responds to both `clearcore_state` generation increments and `SIGUSR1` signals.
5. **Zero Allocations in Audio Path:** The PipeWire capture callback executes zero heap allocations during audio processing, verified via linker wrap tests.
6. **Smooth State Transitions:** Profile updates preserve GRU hidden state tensors and take effect at 10 ms hop boundaries without audio dropouts or clicks.
7. **Honest UI Telemetry:** The Electron UI accurately reports whether voice profile conditioning is active in the native virtual microphone.
8. **Clean Offline Verification:** All automated unit and integration tests pass, and `./scripts/check-offline.sh` reports clean compliance.
