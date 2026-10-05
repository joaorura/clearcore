# Voice Profile Runtime Activation (Stage 1) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Repair the broken TypeScript build and make a stored voice profile actually apply to the live Rust inference backend, with honest `is_voice_profile_active` reporting.

**Architecture:** Add `set_voice_profile` to the `InferenceBackend` trait (explicit policy per impl), delegate through `Box`/`StudioBackend`, hold the applied profile in `EngineSupervisor`, queue updates for `DenoiseEngine`'s worker at the frame boundary, and make `ServiceDaemon` set/clear transactional (apply -> persist -> report, with rollback).

**Tech Stack:** Rust 1.90.0 (workspace crates `model`, `supervisor`, `engine`, `service`), TypeScript/Vitest in `crates/app-tauri`.

**Spec:** `docs/superpowers/specs/2026-10-05-voice-profile-activation-and-runtime-guidance-design.md`

**Out of this plan (separate plans, do not start here):** Stage 2 native path (`filter-capi` + Linux helper; the helper's 16-byte shared state cannot carry a profile, needs its own design); M3 development-asset hash/member verification; runtime-guidance UI states. Nothing here may be called "product end-to-end" or "production approved" (spec section 7).

## Global Constraints

- Shell setup before any cargo command: `export PATH=$HOME/.cargo/bin:$PATH` (cargo is not on PATH by default). Always `--offline --locked`.
- Rust toolchain pinned to `1.90.0` (`rust-toolchain.toml`); `unsafe_code` policy unchanged; no new dependencies.
- Profile validation/FiLM-EQ preparation never runs in a native audio callback; updates apply "at its serialized processing boundary" / "between frames".
- `is_voice_profile_active` is true only when the current backend confirmed application. `active_voice_profile_id` is the applied profile, not merely the stored file.
- Unsupported backends reject `Some(profile)` with a typed unsupported-feature error and accept `None` as a no-op.
- Profiles persisted with mode `0600`, atomically (existing `save_to_file_secure`); profile payloads never logged.
- M3 assets stay development-only; M2 is `NO-GO`.
- Commit messages end with the trailers required by the session attribution reminder.

## File Structure

- `crates/model/src/error.rs` — add `InferenceError::UnsupportedFeature(String)`.
- `crates/model/src/lib.rs` — trait method, `Box<T>` delegation, helper `reject_unsupported_voice_profile`.
- `crates/model/src/studio_backend.rs` — delegate to `inner`.
- `crates/model/src/tract_backend.rs` — trait impl calls existing inherent method.
- `crates/model/src/dsp_pipeline.rs`, `crates/accelerators/src/{ryzenai,tensorrt,openvino,vulkan,coreml,directml,auto}.rs` — unsupported policy.
- Test backends: `studio_backend.rs:141`, `engine.rs:416`, `worker.rs:202`, `crates/engine/tests/{deadline,generation,studio,backpressure}.rs` — explicit policy.
- `crates/supervisor/src/supervisor.rs` — `active_profile`, `set_voice_profile`, re-apply on backend swap.
- `crates/engine/src/{engine,worker}.rs` — `VoiceProfileUpdate`, pending slot, apply before the timed region.
- `crates/service/src/lib.rs` — transactional set/clear, startup apply, status fields.
- `crates/app-tauri/src/{HardwareAcceleratorCard.tsx,VoiceProfileCard.tsx,i18n/types.ts}` — build repair.

---

### Task 1: Repair the TypeScript build

**Files:**
- Modify: `crates/app-tauri/src/HardwareAcceleratorCard.tsx:20,95,98`
- Modify: `crates/app-tauri/src/VoiceProfileCard.tsx:397-410`
- Modify: `crates/app-tauri/src/i18n/types.ts:150-181`
- Test: `crates/app-tauri/src/__tests__/voiceProfileIpc.test.ts`

**Interfaces:**
- Produces: `normalizeVoiceProfileStatus(res: unknown): VoiceProfileStatus` exported from `VoiceProfileCard.tsx` (pure function so it is testable).

- [ ] **Step 1: Confirm the failure**

Run (in `crates/app-tauri`): `npm run build`
Expected: FAIL with TS6133 (`copied`), TS2339 (`active_samples_count`), two TS2353 (`librarySearchedTitle`).

- [ ] **Step 2: Write the failing test for envelope normalization**

Append to `src/__tests__/voiceProfileIpc.test.ts`:

```ts
import { normalizeVoiceProfileStatus } from '../VoiceProfileCard';

describe('normalizeVoiceProfileStatus', () => {
  const base = { is_enrolled: true, active_samples_count: 3, embedding_dim: 192, neural_eq_calibrated: false };
  it('reads a flat status', () => {
    expect(normalizeVoiceProfileStatus(base).active_samples_count).toBe(3);
  });
  it('reads a nested-only envelope', () => {
    expect(normalizeVoiceProfileStatus({ success: true, profile: base }).active_samples_count).toBe(3);
  });
  it('defaults safely for an empty envelope', () => {
    const s = normalizeVoiceProfileStatus({ success: false });
    expect(s.is_enrolled).toBe(false);
    expect(s.active_samples_count).toBe(0);
  });
});
```

Run: `npm test -- --run src/__tests__/voiceProfileIpc.test.ts`
Expected: FAIL (`normalizeVoiceProfileStatus` is not exported).

- [ ] **Step 3: Implement normalization**

In `VoiceProfileCard.tsx`, export next to the component (`VoiceProfileStatus` is in `types.ts:58-64`):

```ts
export function normalizeVoiceProfileStatus(res: unknown): VoiceProfileStatus {
  const r = (res ?? {}) as Partial<VoiceProfileStatus> & { profile?: Partial<VoiceProfileStatus> };
  const src = r.profile ?? r;
  return {
    is_enrolled: Boolean(src.is_enrolled),
    active_samples_count: src.active_samples_count ?? 0,
    embedding_dim: src.embedding_dim ?? 0,
    neural_eq_calibrated: Boolean(src.neural_eq_calibrated),
    gain_boost_db: src.gain_boost_db,
  };
}
```

Replace lines 398-400 with `const prof = normalizeVoiceProfileStatus(await invokeBridge<unknown>('get_voice_profile'));` (keep the surrounding variable names; line 405 then type-checks).

- [ ] **Step 4: Remove the orphaned state**

In `HardwareAcceleratorCard.tsx` delete line 20 (`const [copied, setCopied] ...`) and the two writes `setCopied(true);` (line 95) and `setCopied(false);` (line 98). Keep `copiedKey`.

- [ ] **Step 5: Extend the locale contract**

In `i18n/types.ts`, inside `hardwareBackend` (before `refreshingBtn: string;`) add:

```ts
    librarySearchedTitle: string;
    librarySearchedDesc: string;
    officialDocsTitle: string;
    officialDocsDesc: string;
    openDocLink: string;
    diagnosticGuideTitle: string;
    diagnosticGuideDesc: string;
    nativeCommandsTitle: string;
    copyCommandShort: string;
    recheckBtn: string;
```

- [ ] **Step 6: Verify**

Run: `npm run build && npm test -- --run && npm run test:electron-state`
Expected: build PASS; Vitest all green (previously 42); electron-state green (previously 14).

- [ ] **Step 7: Commit**

```bash
git add crates/app-tauri/src
git commit -m "fix(app): repair TypeScript build (orphaned copied state, IPC envelope, locale contract)"
```

---

### Task 2: Backend contract and explicit unsupported policy

**Files:**
- Modify: `crates/model/src/error.rs:4-16` (+ the exhaustive `Display` match)
- Modify: `crates/model/src/lib.rs:123-143`
- Modify: `crates/model/src/tract_backend.rs:235`
- Modify (add policy): `dsp_pipeline.rs:371`, `accelerators/src/{ryzenai.rs:163,tensorrt.rs:253,openvino.rs:391,vulkan.rs:137,coreml.rs:153,directml.rs:138,auto.rs:309}`
- Modify (test backends): `model/src/studio_backend.rs:141`, `engine/src/engine.rs:416`, `engine/src/worker.rs:202`, `engine/tests/{deadline.rs:80,generation.rs:81,studio.rs:27,backpressure.rs:33}`
- Test: `crates/model/src/lib.rs` (`#[cfg(test)] mod tests`)

**Interfaces:**
- Produces:
  - `InferenceError::UnsupportedFeature(String)`
  - `InferenceBackend::set_voice_profile(&mut self, profile: Option<&VoiceProfile>) -> Result<(), InferenceError>` (required method, no default)
  - `pub fn reject_unsupported_voice_profile(profile: Option<&VoiceProfile>) -> Result<(), InferenceError>` in `model` (`None` -> `Ok`, `Some` -> `UnsupportedFeature("voice profile conditioning")`).

- [ ] **Step 1: Write the failing test**

In `lib.rs` tests:

```rust
#[test]
fn unsupported_policy_accepts_none_and_rejects_some() {
    assert!(reject_unsupported_voice_profile(None).is_ok());
    let p = VoiceProfile::identity("id".into(), "n".into(), "2026-10-05T00:00:00Z".into()).unwrap();
    let err = reject_unsupported_voice_profile(Some(&p)).unwrap_err();
    assert!(matches!(err, InferenceError::UnsupportedFeature(_)));
}
```

Run: `cargo test --offline --locked -p realtime-noise-model --features tract --lib unsupported_policy`
Expected: FAIL (does not compile: items undefined).

- [ ] **Step 2: Add the error variant**

In `error.rs` add `UnsupportedFeature(String),` to `InferenceError` and a `Display` arm: `Self::UnsupportedFeature(f) => write!(fmt, "unsupported feature: {f}"),` (match the existing arm style; the match is exhaustive, so also fix any other exhaustive match the compiler reports).

- [ ] **Step 3: Add the trait method, helper and `Box` delegation**

```rust
pub trait InferenceBackend: Send {
    fn descriptor(&self) -> BackendDescriptor;
    fn process(&mut self, input: &AudioFrame) -> Result<ProcessedFrame, InferenceError>;
    fn algorithmic_latency_samples(&self) -> u32;
    fn set_voice_profile(&mut self, profile: Option<&VoiceProfile>) -> Result<(), InferenceError>;
}

pub fn reject_unsupported_voice_profile(profile: Option<&VoiceProfile>) -> Result<(), InferenceError> {
    match profile {
        None => Ok(()),
        Some(_) => Err(InferenceError::UnsupportedFeature("voice profile conditioning".into())),
    }
}
```

In the `Box<T>` impl add: `fn set_voice_profile(&mut self, profile: Option<&VoiceProfile>) -> Result<(), InferenceError> { (**self).set_voice_profile(profile) }`.

- [ ] **Step 4: Implement every impl**

- `TractBackend` (`tract_backend.rs:235` impl block): `fn set_voice_profile(&mut self, profile: Option<&VoiceProfile>) -> Result<(), InferenceError> { TractBackend::set_voice_profile(self, profile) }` (explicit path calls the existing inherent method at `:152`).
- Every other production impl (`AgnosticDspBackend`, the six accelerators, `MockTractBackend`): `{ crate::reject_unsupported_voice_profile(profile) }` — in `accelerators` use `realtime_noise_model::reject_unsupported_voice_profile(profile)`.
- Test backends: same unsupported policy (deterministic, never claims support).

- [ ] **Step 5: Run to verify**

Run: `cargo test --offline --locked --workspace --features tract --no-run` then the focused test from Step 1.
Expected: workspace compiles; focused test PASS.

- [ ] **Step 6: Commit**

```bash
git add crates
git commit -m "feat(model): add set_voice_profile to InferenceBackend with explicit unsupported policy"
```

---

### Task 3: StudioBackend delegation

**Files:**
- Modify: `crates/model/src/studio_backend.rs:82` (impl) and tests near `:141`
- Test: same file (`FakeBackend`)

**Interfaces:**
- Consumes: Task 2 trait method.
- Produces: `StudioBackend<B>::set_voice_profile` forwarding to `self.inner`.

- [ ] **Step 1: Write the failing test**

Give `FakeBackend` a field `profile_calls: usize` and make its `set_voice_profile` increment it and return `Ok(())`. Test:

```rust
#[test]
fn studio_backend_delegates_profile_to_inner() {
    let mut s = StudioBackend::new(FakeBackend { fail: false, profile_calls: 0 /* + existing ctor args */ });
    let p = VoiceProfile::identity("id".into(), "n".into(), "2026-10-05T00:00:00Z".into()).unwrap();
    s.set_voice_profile(Some(&p)).unwrap();
    s.set_voice_profile(None).unwrap();
    assert_eq!(s.inner_ref().profile_calls, 2);
}
```

(Use the existing `StudioBackend` constructor already used by neighboring tests; if there is no accessor to `inner`, assert through a shared `Arc<AtomicUsize>` stored in `FakeBackend` instead.)

Run: `cargo test --offline --locked -p realtime-noise-model --features tract --lib studio_backend_delegates`
Expected: FAIL (stub returns without forwarding, or counter 0).

- [ ] **Step 2: Implement**

In the `StudioBackend<B>` impl: `fn set_voice_profile(&mut self, profile: Option<&VoiceProfile>) -> Result<(), InferenceError> { self.inner.set_voice_profile(profile) }`

- [ ] **Step 3: Verify and commit**

Run: `cargo test --offline --locked -p realtime-noise-model --features tract --lib studio_backend` — Expected: PASS.

```bash
git add crates/model
git commit -m "feat(model): StudioBackend forwards voice profile to inner backend"
```

---

### Task 4: EngineSupervisor holds and applies the profile

**Files:**
- Modify: `crates/supervisor/src/supervisor.rs:59-70` (struct), `:179` (`set_backend`), `:193` (`select_backend`)
- Test: `crates/supervisor/tests/voice_profile.rs` (create)

**Interfaces:**
- Consumes: Task 2/3.
- Produces on `EngineSupervisor`:
  - `pub fn set_voice_profile(&mut self, profile: Option<&VoiceProfile>) -> Result<(), InferenceError>`
  - `pub fn active_voice_profile_id(&self) -> Option<&str>`
  - `pub fn active_voice_profile(&self) -> Option<&VoiceProfile>`

Behavior contract: no backend -> `Err(UnsupportedFeature("no active backend"))`; backend error -> previous profile preserved and still reported; success -> `active_profile` updated *after* backend success. On `set_backend`/`select_backend` swap, the stored profile is re-applied to the new backend; if re-apply fails, `active_profile` becomes `None` (never report a profile the live backend did not confirm).

- [ ] **Step 1: Write the failing tests**

Create `crates/supervisor/tests/voice_profile.rs` with a test backend. Copy the `descriptor()` body from `crates/engine/tests/studio.rs:27` (`Passthrough`) and use these differences:

```rust
struct ProfileBackend { supports: bool }
// process / descriptor / latency as Passthrough
fn set_voice_profile(&mut self, p: Option<&VoiceProfile>) -> Result<(), InferenceError> {
    if self.supports { Ok(()) } else { realtime_noise_model::reject_unsupported_voice_profile(p) }
}

fn profile(id: &str) -> VoiceProfile {
    VoiceProfile::identity(id.into(), "n".into(), "2026-10-05T00:00:00Z".into()).unwrap()
}

#[test]
fn applied_profile_is_reported_only_after_backend_success() {
    let mut sup = EngineSupervisor::new();
    sup.set_backend(Box::new(ProfileBackend { supports: true }), "fake");
    sup.set_voice_profile(Some(&profile("a"))).unwrap();
    assert_eq!(sup.active_voice_profile_id(), Some("a"));
    sup.set_voice_profile(None).unwrap();
    assert_eq!(sup.active_voice_profile_id(), None);
}

#[test]
fn unsupported_backend_never_reports_active_and_keeps_previous() {
    let mut sup = EngineSupervisor::new();
    sup.set_backend(Box::new(ProfileBackend { supports: false }), "fake");
    assert!(sup.set_voice_profile(Some(&profile("a"))).is_err());
    assert_eq!(sup.active_voice_profile_id(), None);
}

#[test]
fn no_backend_is_an_error() {
    let mut sup = EngineSupervisor::new();
    assert!(sup.set_voice_profile(Some(&profile("a"))).is_err());
}

#[test]
fn backend_swap_to_unsupported_drops_the_active_profile() {
    let mut sup = EngineSupervisor::new();
    sup.set_backend(Box::new(ProfileBackend { supports: true }), "a");
    sup.set_voice_profile(Some(&profile("a"))).unwrap();
    sup.set_backend(Box::new(ProfileBackend { supports: false }), "b");
    assert_eq!(sup.active_voice_profile_id(), None);
}
```

(Use whatever constructor `EngineSupervisor` offers, as in `tests/supervision.rs`.)

Run: `cargo test --offline --locked -p realtime-noise-supervisor --test voice_profile`
Expected: FAIL (methods missing).

- [ ] **Step 2: Implement**

Add field `active_profile: Option<VoiceProfile>` (init `None` in the constructor), then:

```rust
pub fn set_voice_profile(&mut self, profile: Option<&VoiceProfile>) -> Result<(), InferenceError> {
    let backend = self.backend.as_mut()
        .ok_or_else(|| InferenceError::UnsupportedFeature("no active backend".into()))?;
    backend.set_voice_profile(profile)?;
    self.active_profile = profile.cloned();
    Ok(())
}
pub fn active_voice_profile(&self) -> Option<&VoiceProfile> { self.active_profile.as_ref() }
pub fn active_voice_profile_id(&self) -> Option<&str> { self.active_profile.as_ref().map(|p| p.id.as_str()) }

fn reapply_voice_profile(&mut self) {
    if let Some(p) = self.active_profile.take() {
        if self.set_voice_profile(Some(&p)).is_err() {
            self.active_profile = None;
        }
    }
}
```

Call `self.reapply_voice_profile();` at the end of `set_backend` (`:179`) and wherever `select_backend` (`:193`) installs a new backend (it may delegate to `set_backend`; call once).

- [ ] **Step 3: Verify and commit**

Run: `cargo test --offline --locked -p realtime-noise-supervisor` — Expected: PASS, existing tests unchanged.

```bash
git add crates/supervisor
git commit -m "feat(supervisor): apply and track the active voice profile across backend swaps"
```

---

### Task 5: DenoiseEngine applies updates at the frame boundary

**Files:**
- Modify: `crates/engine/src/engine.rs` (`EngineSharedState` ~`:112-122`, `DenoiseEngine`)
- Modify: `crates/engine/src/worker.rs:52-55` (apply site)
- Modify: `crates/engine/src/lib.rs` (export)
- Test: `crates/engine/tests/voice_profile.rs` (create)

**Interfaces:**
- Produces:
  - `pub enum VoiceProfileUpdate { Set(VoiceProfile), Clear }` (exported)
  - `DenoiseEngine::set_voice_profile(&mut self, update: VoiceProfileUpdate) -> Result<(), EngineError>`
  - `DenoiseEngine::applied_voice_profile_id(&self) -> Option<String>`
  - `DenoiseEngine::voice_profile_error(&self) -> Option<String>`

Behavior: when not running, apply synchronously to `state.backend` (error -> `EngineError::BackendError`). When running, store in `pending_voice_profile: Option<VoiceProfileUpdate>` (latest wins). The worker takes it under the same lock where it applies `pending_backend` (`worker.rs:52-55`), **before** `let start = Instant::now()` so the blocking Tract round-trip never counts toward the 10 ms deadline; result is recorded into `applied_voice_profile_id` / `voice_profile_error`. A failed update leaves the previous profile id unchanged.

- [ ] **Step 1: Write the failing tests**

In `tests/voice_profile.rs` reuse the `ProfileBackend` shape from Task 4 (record calls in `Arc<Mutex<Vec<Option<String>>>>`):

```rust
#[test]
fn stopped_engine_applies_synchronously() { /* set Set(profile "a") -> applied_voice_profile_id()==Some("a"); Clear -> None */ }

#[test]
fn running_engine_applies_between_frames_not_inside_set() {
    // start engine with BoundedQueueTransport (see tests/backpressure.rs), call set_voice_profile(Set(a)),
    // assert recorded calls is empty immediately; push one hop, wait until output pops,
    // assert recorded calls == [Some("a")] and applied_voice_profile_id()==Some("a")
}

#[test]
fn failed_update_keeps_previous_profile_id() {
    // backend supports=true applies "a"; flip supports=false via a shared AtomicBool; Set(b) fails;
    // after a hop: applied id still "a", voice_profile_error() is Some(_)
}
```

Run: `cargo test --offline --locked -p realtime-noise-engine --test voice_profile`
Expected: FAIL (API missing).

- [ ] **Step 2: Implement**

Add to `EngineSharedState`: `pending_voice_profile: Option<VoiceProfileUpdate>`, `applied_voice_profile_id: Option<String>`, `voice_profile_error: Option<String>`. Add a shared helper in `engine.rs`:

```rust
pub(crate) fn apply_profile_update(
    state: &mut EngineSharedState,
    update: VoiceProfileUpdate,
) {
    let Some(backend) = state.backend.as_mut() else {
        state.voice_profile_error = Some("no active backend".into());
        return;
    };
    let (arg, id) = match &update {
        VoiceProfileUpdate::Set(p) => (Some(p), Some(p.id.clone())),
        VoiceProfileUpdate::Clear => (None, None),
    };
    match backend.set_voice_profile(arg) {
        Ok(()) => { state.applied_voice_profile_id = id; state.voice_profile_error = None; }
        Err(e) => state.voice_profile_error = Some(e.to_string()),
    }
}
```

`DenoiseEngine::set_voice_profile`: lock; if `!state.is_running` call `apply_profile_update` then return `Err(BackendError(..))` if `voice_profile_error` is set, else `Ok`; otherwise `state.pending_voice_profile = Some(update)`. In `worker.rs` right after `pending_backend` is applied: `if let Some(u) = state.pending_voice_profile.take() { apply_profile_update(&mut state, u); }`. When `pending_backend` is swapped in, also re-apply the engine's remembered profile only if the spec'd caller (service) asks — not here (YAGNI).

- [ ] **Step 3: Verify and commit**

Run: `cargo test --offline --locked -p realtime-noise-engine` — Expected: PASS including `deadline.rs`, `generation.rs`.

```bash
git add crates/engine
git commit -m "feat(engine): apply voice profile updates at the worker frame boundary"
```

---

### Task 6: Transactional service set/clear, startup apply, truthful status

**Files:**
- Modify: `crates/service/src/lib.rs` (struct `:31-44`, `attach_profile_store :140-154`, `GetStatus :249-281`, dispatch `:377-385`, free fns `:590-626`)
- Modify: `crates/service/tests/voice_profile.rs` (adjust existing assertions on `is_voice_profile_active`; add tests)

**Interfaces:**
- Consumes: Task 4 `EngineSupervisor::{set_voice_profile, active_voice_profile, active_voice_profile_id}`.
- Produces: status JSON fields — `active_voice_profile_id` (applied), `stored_voice_profile_id` (on disk), `is_voice_profile_active` (applied), `voice_profile_selected` (stored exists), `voice_profile_error: Option<String>`.

Transaction (set): parse+verify (`VoiceProfile::from_json`) -> remember `previous = supervisor.active_voice_profile().cloned()` -> `supervisor.set_voice_profile(Some(&new))` (error -> `internal_error`/unsupported response, disk untouched) -> `store.save_active(&new)` (error -> `supervisor.set_voice_profile(previous.as_ref())` rollback, return `PERSIST_FAILED`) -> update `stored_voice_profile_id`. Clear: `supervisor.set_voice_profile(None)` -> `store.clear_active()` (error -> restore previous) -> clear ids. Response payload keeps `{"active_voice_profile_id": ...}`.

Startup (`attach_profile_store`): `load_active()`; on `Ok(Some(p))` set `stored_voice_profile_id = p.id`, then try `supervisor.set_voice_profile(Some(&p))`; on failure keep stored, record `voice_profile_error`, do not delete the file, audio stays neutral. Also re-attempt in the backend-switch handler (`SetBackend`, `:293`) is already covered by Task 4's re-apply; after it, refresh nothing (status reads from supervisor).

- [ ] **Step 1: Write the failing tests** (in `tests/voice_profile.rs`, using helpers `daemon_with_store`, `set_profile`, `send`, `test_profile`; add a supervisor backend that supports profiles via `daemon.supervisor_mut().set_backend(...)` — if no accessor exists, use the one the other service tests use in `tests/backend_lifecycle.rs`)

```rust
#[test] fn set_applies_to_backend_then_persists_and_reports_active() { /* supports=true: status.is_voice_profile_active==true, active id==stored id, file 0600 */ }
#[test] fn unsupported_backend_rejects_and_leaves_disk_and_state_untouched() { /* supports=false: response is error, no active_profile.json, active false */ }
#[test] fn persistence_failure_rolls_back_to_previous_backend_profile() { /* set "a" ok; make dir read-only; set "b" fails; backend recorded calls end with Some("a"); active id == "a" */ }
#[test] fn clear_applies_neutral_then_removes_file() { /* active false, file gone, backend saw None */ }
#[test] fn restart_distinguishes_stored_from_active() { /* restart with unsupported backend: stored_voice_profile_id==Some, is_voice_profile_active==false, file kept */ }
#[test] fn restart_with_supporting_backend_restores_active() { /* active true after attach */ }
```

Run: `cargo test --offline --locked -p realtime-noise-service --test voice_profile`
Expected: FAIL.

- [ ] **Step 2: Implement**

Replace `active_voice_profile_id` field with `stored_voice_profile_id: Option<String>` and `voice_profile_error: Option<String>`; make `set_voice_profile`/`clear_voice_profile` methods (or pass `&mut EngineSupervisor` into the free functions) following the transaction above; in `GetStatus` replace the hardcoded block at `:265-270` with:

```rust
"active_voice_profile_id": self.supervisor.active_voice_profile_id(),
"stored_voice_profile_id": self.stored_voice_profile_id,
"voice_profile_selected": self.stored_voice_profile_id.is_some(),
"is_voice_profile_active": self.supervisor.active_voice_profile_id().is_some(),
"voice_profile_error": self.voice_profile_error,
```

Remove the outdated comment at `:266-268`. Update `active_voice_profile_id(&self)` accessor at `:157` to read from the supervisor.

- [ ] **Step 3: Fix pre-existing tests**

Existing tests in `voice_profile.rs` that asserted `is_voice_profile_active == false` after a successful set must now provide a supporting backend or assert the new truthful value; do not weaken the "unsupported never active" assertions.

- [ ] **Step 4: Verify the whole Rust workspace**

Run:
```bash
export PATH=$HOME/.cargo/bin:$PATH
cargo fmt --all -- --check
cargo clippy --offline --locked --workspace --features tract -- -D warnings
cargo test --offline --locked --workspace --features tract
./scripts/check-offline.sh
```
Expected: all PASS. Report literal output; if the local toolchain cannot run a step, say so instead of skipping silently.

- [ ] **Step 5: Commit**

```bash
git add crates/service
git commit -m "feat(service): transactional voice profile set/clear with truthful active status"
```

---

### Task 7: Frontend status honesty

**Files:**
- Modify: `crates/app-tauri/src/types.ts:58-64`, `VoiceProfileCard.tsx`, `electron/main.cjs` (status passthrough), both locale files
- Test: `crates/app-tauri/src/__tests__/voiceProfileIpc.test.ts`

**Interfaces:**
- Consumes: Task 6 status fields `is_voice_profile_active`, `stored_voice_profile_id`, `voice_profile_error`.

- [ ] **Step 1: Write the failing test** — a stored-but-not-active status renders a "stored, not applied" label and never the "active" label; an active status renders "active".
- [ ] **Step 2: Implement** — add optional fields to `VoiceProfileStatus`, add i18n keys `voiceProfile.storedNotApplied` to `Translations`, `en-US.ts` and `pt-BR.ts` (the recursive locale-parity test enforces both), render by the rules above.
- [ ] **Step 3: Verify** — `npm run build && npm test -- --run && npm run test:electron-state`. Expected: PASS.
- [ ] **Step 4: Commit** — `git commit -m "feat(app): distinguish stored from applied voice profile in the UI"`

---

## Self-Review

- **Spec coverage:** 3.1 -> Tasks 2-3; 3.2 -> Tasks 4-5; 3.3 -> Task 6; 5 (status semantics) -> Tasks 6-7; section 6 order items 1-5 and 8 -> Tasks 2-7 and 1. Deferred, by design and stated at the top: 3.4 stage 2 (native), 3.5 (M3 assets), 3.6 (enrollment denoise default is already raw; no code here), 3.7 (runtime guidance UI).
- **Known uncertainty to resolve while executing:** exact `EngineSupervisor`/`StudioBackend` constructor names and whether `DenoiseEngine` has `is_running` as a field vs `state` enum (the code map shows both a `state` and `is_running: bool` on `EngineSharedState`); the plan names them as mapped, the executor must match the compiler.
- **Not verified yet:** none of these tasks has been run; `cargo` was only confirmed to work offline via `cargo check` on the service crate.
