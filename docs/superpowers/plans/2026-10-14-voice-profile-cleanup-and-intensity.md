# Voice Profile Cleanup Synchronization, Environmental Noise Quality Gate, and Isolation Intensity Slider Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix voice profile deactivation synchronization so clearing a profile or turning off voice isolation immediately reverts the running audio engine to neutral mode, introduce environmental noise rejection during sample ingestion, add a speech cutoff troubleshooting banner, and implement a voice isolation intensity control combining preset anchor buttons and a slider (0–100%).

**Architecture:** 
1. Linux shared state generation bumping in both Electron main process and Rust daemon upon profile clear/build/toggle, coupled with setting-aware `clearcore_filter_reload_active_profile` in `filter-capi` to guarantee immediate zero-delay fallback to neutral unconditioned DeepFilterNet3.
2. Ingestion-time SNR validation comparing denoised speech to estimated noise in `enrollment_ingest.rs` to reject heavily contaminated takes with `ENROLL_NOISY_ENVIRONMENT`.
3. FiLM vector linear interpolation based on `voice_isolation_intensity` (0..100) in `voice_profile.rs` / `ort_backend.rs` / `tract_backend.rs` and daemon settings.
4. UI enhancement in `ProfilePanel.tsx` providing a troubleshooting alert and a combined preset buttons + slider control for voice isolation intensity.

**Tech Stack:** Rust (service, filter-capi, model), C (PipeWire helper shared state), TypeScript / React / Electron (app-tauri).

## Global Constraints
- Realtime digital silence for rejected speech is strictly preserved (no attenuation floor).
- Zero-allocation in realtime audio paths.
- Offline-first execution: all validations and models operate 100% locally.

---

### Task 1: Shared State Generation Bump and Filter Neutral Fallback

**Files:**
- Modify: `crates/app-tauri/electron/clearcore-state.cjs`
- Modify: `crates/app-tauri/electron/main.cjs`
- Modify: `crates/service/src/lib.rs`
- Modify: `crates/filter-capi/src/lib.rs`
- Test: `crates/app-tauri/scripts/clearcore-state.selftest.cjs`
- Test: `crates/filter-capi/src/lib.rs` (unit tests)

- [ ] **Step 1: Write/update test verifying generation increment and settings check on reload**
- [ ] **Step 2: Implement generation increment helper in `clearcore-state.cjs` and trigger it in `main.cjs` on `set_voice_profile` unenroll/clear and `set_voice_isolation`**
- [ ] **Step 3: Implement Linux shared state generation bumper in `crates/service/src/lib.rs` on `ClearVoiceProfile`, `SetVoiceIsolation`, and `apply_profile`**
- [ ] **Step 4: Update `clearcore_filter_reload_active_profile` in `crates/filter-capi/src/lib.rs` to inspect `settings.json` `voice_isolation_enabled` and return neutral (0) if disabled or missing**
- [ ] **Step 5: Verify unit tests pass**

---

### Task 2: Environmental Noise Quality Gate in Sample Ingestion

**Files:**
- Modify: `crates/ipc/src/enrollment_codes.rs`
- Modify: `crates/service/src/enrollment_error.rs`
- Modify: `crates/service/src/enrollment_ingest.rs`
- Modify: `crates/app-tauri/src/voice/enrollmentErrors.ts`
- Modify: `crates/app-tauri/src/i18n/locales/pt-BR.ts`
- Modify: `crates/app-tauri/src/i18n/locales/en-US.ts`
- Test: `crates/service/tests/enrollment_ingest.rs` or unit test in `enrollment_ingest.rs`

- [ ] **Step 1: Add `ENROLL_NOISY_ENVIRONMENT` code in `realtime-noise-ipc` and `EnrollError::NoisyEnvironment` in `service`**
- [ ] **Step 2: Calculate active speech SNR in `enrollment_ingest.rs` comparing clean speech energy against residual noise energy, rejecting if SNR < 6.0 dB**
- [ ] **Step 3: Add localization strings in pt-BR and en-US for `ENROLL_NOISY_ENVIRONMENT`**
- [ ] **Step 4: Verify tests pass**

---

### Task 3: Voice Isolation Intensity Setting & FiLM Vector Scaling

**Files:**
- Modify: `crates/service/src/settings.rs`
- Modify: `crates/ipc/src/protocol.rs`
- Modify: `crates/service/src/lib.rs`
- Modify: `crates/model/src/voice_profile.rs`
- Modify: `crates/model/src/ort_backend.rs`
- Modify: `crates/model/src/tract_backend.rs`
- Modify: `crates/filter-capi/src/lib.rs`
- Test: `crates/model/src/voice_profile.rs` (unit tests)
- Test: `crates/service/tests/settings.rs`

- [ ] **Step 1: Add `with_intensity(&self, intensity: u8) -> FiLMVectors` to `FiLMVectors` with linear scaling ($\gamma = 1.0 + \alpha(\gamma - 1.0), \beta = \alpha \beta$)**
- [ ] **Step 2: Add `voice_isolation_intensity: u8` (default 100) to `Settings` in `crates/service/src/settings.rs` and IPC command `SetVoiceIsolationIntensity { intensity: u8 }`**
- [ ] **Step 3: Apply intensity scaling in supervisor / backends / `filter-capi` when loading profile**
- [ ] **Step 4: Verify unit tests pass**

---

### Task 4: UI Voice Isolation Controls and Troubleshooting Banner

**Files:**
- Modify: `crates/app-tauri/src/voice/panels/ProfilePanel.tsx`
- Modify: `crates/app-tauri/src/bridge.ts`
- Modify: `crates/app-tauri/electron/main.cjs`
- Modify: `crates/app-tauri/src/i18n/locales/pt-BR.ts`
- Modify: `crates/app-tauri/src/i18n/locales/en-US.ts`
- Test: `crates/app-tauri/src/voice/__tests__/panels.test.tsx`

- [ ] **Step 1: Add IPC handlers in `main.cjs` and bridge functions in `bridge.ts` for `get_voice_isolation_intensity` and `set_voice_isolation_intensity`**
- [ ] **Step 2: Add troubleshooting alert banner in `ProfilePanel.tsx` when voice profile is active**
- [ ] **Step 3: Add Voice Isolation Intensity control in `ProfilePanel.tsx` combining preset anchor buttons (e.g. 50% Moderado, 75% Firme, 100% Máximo) and slider (0-100%)**
- [ ] **Step 4: Update test suites in `crates/app-tauri` and run `npm test`**

---

### Task 5: End-to-End Verification and Release Beta 1 Regeneration

**Files:**
- Run full test suite: `cargo test --workspace`
- Run frontend test suite: `cd crates/app-tauri && npm test`
- Package release: `./package.sh`
- Verify git status, update git tag `v0.1.0-beta.1` and release commit.
