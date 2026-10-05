# Voice Enrollment Pipeline — App (Electron + React) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. **Waves 1 and 2 run their tasks in parallel, one worktree each; the ownership table is what makes that safe — never edit a file another task owns. `VoiceProfileCard.tsx` is a 1,529-line component: only Task E2 may edit it.**

**Goal:** Record the raw physical microphone as 48 kHz PCM, send it to the service, show quality and the 90 s speech budget, make the user delete audio when the budget is exceeded, and build/apply the profile — with no fake data on screen.

**Architecture:** New isolated modules under `src/voice/` (capture, client, budget, errors, presentational components) and one new Electron module (`electron/enrollment-ipc.cjs`, pure and unit-testable). `VoiceProfileCard.tsx` and `main.cjs` change only in the final integration tasks, which touch disjoint files and can run in parallel.

**Tech Stack:** TypeScript/React/Vite, Vitest (node environment, no DOM), Electron main process in CommonJS, `node:assert` selftests.

**Spec:** `docs/superpowers/specs/2026-10-05-voice-enrollment-pipeline-design.md` (§2 D1–D8, §4.4, §5, §6.1, §9). Service side: `2026-10-05-voice-enrollment-pipeline-service.md` (its Task 0 fixes the IPC contract this plan consumes).

## Global Constraints

- Work in `crates/app-tauri`. Commands: `npm run build`, `npm test -- --run`, `npm run test:electron-state`, `node scripts/<name>.selftest.cjs`. Vitest runs in **node** (no `document`/`window`): test pure logic and render components with `renderToStaticMarkup` from `react-dom/server`.
- **No new npm dependency.** No `jsdom`, no `@testing-library`.
- D1: capture **only the raw physical microphone** (echoCancellation/noiseSuppression/autoGainControl off); never the virtual/loopback device; **no fallback** to an unconstrained `getUserMedia`; if no physical device opens, show an error. D7: send the device label and a stable hash with every sample.
- No fabricated data: remove `createSyntheticAudioUrl`, the `Math.random()` VU fallback that keeps "recording", `INITIAL_CALL_TAKES` mocks, invented `durationSec: 5.0`, and any hard-coded `neural_eq_calibrated: true` / `gain_boost_db: 1.8` / `embedding_dim: 192` shown as status.
- Raw PCM goes renderer → main by IPC as a `Float32Array`/`ArrayBuffer`, is base64-encoded in main, sent once, and **never written to disk, localStorage or logs**.
- UI honesty (spec §9): "applied in service (development)"; a visible notice that the enrollment model is a development model, not approved; no text says the profile improves isolation, "end-to-end" or "production".
- UI strings: Brazilian Portuguese with correct accents, English in `en-US`; every new key exists in `i18n/types.ts`, `pt-BR.ts` and `en-US.ts` (the parity test enforces it) — added **only by Task E2**. Earlier tasks take labels as props typed by `EnrollmentLabels`.
- Keep the existing exported pure helpers importable from `src/VoiceProfileCard.tsx` (tests import them there): re-export from the new modules when moving code.
- Agent preamble: **app tasks run in the main worktree** `/home/joaorura/orca/workspaces/clearcore/hippocamp` (git worktrees do not carry `node_modules`; the file-ownership table keeps parallel tasks from touching the same file). Other agents commit concurrently, including Rust ones in other directories: `git add <your paths>` only (never `-A`, never `.`), no `git stash`, and if a commit fails on `index.lock` wait a few seconds and retry. Run only your own tests while others are mid-edit; if a failure comes from a file you do not own, report it instead of editing it. Trailers: `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>` and `Claude-Session: https://claude.ai/code/session_013NV6Le7wWJGJVHVStPZrBy`.

## Waves and file ownership

| Wave | Task | Owns |
|---|---|---|
| 0 (serial) | **W0 Shared types** | `src/voice/enrollmentTypes.ts` |
| 1 (parallel) | **A1** capture | `src/voice/{captureDevice.ts,pcmCapture.ts,pcmWorklet.ts}`, `src/voice/__tests__/capture*.test.ts` |
| 1 | **B1** Electron IPC module | `electron/enrollment-ipc.cjs`, `scripts/enrollment-ipc.selftest.cjs` |
| 1 | **C1** client, budget, errors | `src/voice/{enrollmentClient.ts,speechBudget.ts,enrollmentErrors.ts}`, `src/voice/__tests__/{client,budget,errors}*.test.ts` |
| 1 | **D1** presentational components | `src/voice/{VoiceBudgetMeter.tsx,VoiceSampleGallery.tsx,BudgetErrorBanner.tsx,EnrollmentJobStatus.tsx,DevModelNotice.tsx}`, `src/voice/__tests__/components.test.tsx` |
| 2 (parallel) | **E1** Electron integration | `electron/main.cjs`, `electron/voice-profile-store.cjs`, `electron/preload.cjs` (if needed), `src/bridge.ts`, `package.json`, `scripts/voice-profile-ipc.selftest.cjs` |
| 2 | **E2** renderer integration | `src/VoiceProfileCard.tsx`, `src/types.ts`, `src/i18n/types.ts`, `src/i18n/locales/{pt-BR,en-US}.ts`, existing tests under `src/__tests__/` |
| 3 | **V2** verification + QA checklist | read-only |

---

## Task W0: Shared types and command names (serial, ~15 minutes)

**File:** `src/voice/enrollmentTypes.ts` (create). Everything below is the contract between tasks and with the service.

```ts
export const MAX_SPEECH_SECONDS = 90;
export const MIN_TAKE_MARGIN_SECONDS = 5;
export const CAPTURE_SAMPLE_RATE = 48_000;
export const MAX_RECORD_SECONDS = 30;                 // per sample, UI guard
export const MIN_RECORD_SECONDS = 1.5;

export type EnrollErrorCode =
  | 'ENROLL_CLIPPING' | 'ENROLL_TOO_QUIET' | 'ENROLL_TOO_LITTLE_SPEECH' | 'ENROLL_MODEL_NOT_CONFIGURED'
  | 'ENROLL_BUDGET_EXCEEDED' | 'ENROLL_INVALID_AUDIO' | 'ENROLL_PAYLOAD_TOO_LARGE'
  | 'ENROLL_JOB_NOT_FOUND' | 'ENROLL_FAILED' | 'SERVICE_UNAVAILABLE';

export interface DeviceInfo { label: string; idHash: string }          // idHash: 64 lowercase hex
export interface CapturedPcm {
  pcm: Float32Array; sampleRate: typeof CAPTURE_SAMPLE_RATE; durationSec: number;
  peak: number; rmsDbfs: number; device: DeviceInfo;
}
export interface SpeechBudget { usedSeconds: number; maxSeconds: number; remainingSeconds: number }
export interface ServiceSample {
  id: string; name: string; timestamp: string; speechSeconds: number; deviceLabel: string;
  usedInProfile: boolean; needsReenroll: boolean; otherMicrophone: boolean;
}
export interface SampleList { samples: ServiceSample[]; budget: SpeechBudget }
export type JobState = 'running' | 'done' | 'failed';
export type JobStage = 'queued' | 'denoise' | 'trim' | 'eq' | 'enroll' | 'apply';
export interface Quality { peak: number; rmsDbfs: number; activeFraction: number; speechSeconds: number }
export interface EnrollmentJob {
  jobId: string; state: JobState; stage: JobStage; errorCode: EnrollErrorCode | null;
  remainingSeconds: number | null; sampleId: string | null; profileId: string | null; quality: Quality | null;
}

/** Renderer ⇄ main command names (all go through invokeBridge → ipcMain.handle). */
export const CMD = {
  addSample: 'enrollment_add_sample',       // { pcm: Float32Array, sampleRate: 48000, name: string, device: DeviceInfo } -> { jobId } | { errorCode }
  buildProfile: 'enrollment_build_profile', // { name: string } -> { jobId } | { errorCode }
  getJob: 'enrollment_get_job',             // { jobId: string } -> EnrollmentJob | { errorCode }
  listSamples: 'enrollment_list_samples',   // {} -> SampleList | { errorCode }
  deleteSample: 'enrollment_delete_sample', // { id: string } -> { success: boolean } | { errorCode }
} as const;

/** Every user-visible string the new components render; E2 fills it from i18n. */
export interface EnrollmentLabels {
  budgetTitle: string; budgetUsed: string; budgetRemaining: string; seconds: string;
  budgetExceededTitle: string; budgetExceededBody: string;        // body has "{remaining}" placeholder
  deleteAction: string; deleting: string;
  otherMicrophone: string; needsReenroll: string; usedInProfile: string; notUsed: string;
  stageQueued: string; stageDenoise: string; stageTrim: string; stageEq: string; stageEnroll: string; stageApply: string;
  jobDone: string; jobFailed: string;
  devModelNotice: string;                                          // "development model, not approved"
  qualityPeak: string; qualityLevel: string; qualitySpeech: string;
  errors: Record<EnrollErrorCode | 'UNKNOWN', string>;
}
```

- [ ] **Step 1:** write `src/voice/__tests__/enrollmentTypes.test.ts`:
```ts
import { CMD, MAX_SPEECH_SECONDS, MIN_TAKE_MARGIN_SECONDS } from '../enrollmentTypes';
it('exposes the agreed constants and unique command names', () => {
  expect(MAX_SPEECH_SECONDS).toBe(90); expect(MIN_TAKE_MARGIN_SECONDS).toBe(5);
  const names = Object.values(CMD); expect(new Set(names).size).toBe(names.length);
  expect(names.every((n) => n.startsWith('enrollment_'))).toBe(true);
});
```
- [ ] **Step 2:** `npm test -- --run src/voice/__tests__/enrollmentTypes.test.ts` → FAIL; **Step 3:** create the file; **Step 4:** PASS + `npm run build`; **Step 5:** `git commit -m "feat(app): fix the enrollment types and command names"`.

---

## Wave 1 (parallel; after W0 is on the branch)

### Task A1: raw PCM capture (`src/voice/captureDevice.ts`, `pcmCapture.ts`, `pcmWorklet.ts`)

**Interfaces:**
```ts
// captureDevice.ts — moved/adapted from VoiceProfileCard.tsx (copy; E2 later re-exports them)
export function isVirtualOrLoopbackAudioDevice(label: string): boolean;
export function resolvePhysicalAudioDevice(inputs: MediaDeviceInfo[], selectedId?: string, all?: MediaDeviceInfo[]): MediaDeviceInfo | undefined;
export class PhysicalMicUnavailableError extends Error {}
export async function sha256Hex(text: string): Promise<string>;
export async function acquireRawPhysicalStream(selectedInputId?: string):
  Promise<{ stream: MediaStream; device: DeviceInfo }>;   // NO fallback: throws PhysicalMicUnavailableError
// pcmCapture.ts
export function concatChunks(chunks: Float32Array[]): Float32Array;
export function measurePcm(pcm: Float32Array): { peak: number; rmsDbfs: number };   // rmsDbfs = -120 for silence
export class PcmRecorder {
  constructor(opts?: { maxSeconds?: number });
  start(stream: MediaStream, onLevel?: (level01: number) => void): Promise<void>;   // AudioContext({sampleRate:48000}) + worklet from a Blob URL
  stop(device: DeviceInfo): Promise<CapturedPcm>;
}
// pcmWorklet.ts
export const PCM_WORKLET_SOURCE: string;   // registerProcessor('pcm-capture', ...) posts Float32Array copies of input[0][0]
```
Constraints used when opening: `{ audio: { deviceId: { exact }, echoCancellation: false, noiseSuppression: false, autoGainControl: false, channelCount: 1, sampleRate: { ideal: 48000 } } }`. `idHash = sha256Hex(deviceId + '|' + groupId)`; `label` is trimmed to 128 chars.

- [ ] **Step 1: Failing tests** (`src/voice/__tests__/capture.test.ts`; mock `navigator.mediaDevices` with `vi.stubGlobal`):
```ts
it('measurePcm reports peak and rms', () => {
  const m = measurePcm(new Float32Array([0.5, -0.5, 0.5, -0.5]));
  expect(m.peak).toBeCloseTo(0.5); expect(m.rmsDbfs).toBeCloseTo(-6.02, 1);
  expect(measurePcm(new Float32Array(4)).rmsDbfs).toBe(-120);
});
it('concatChunks joins in order', () => { expect([...concatChunks([new Float32Array([1,2]), new Float32Array([3])])]).toEqual([1,2,3]); });
it('virtual and loopback labels are excluded', () => {
  for (const l of ['ClearCore Virtual Mic', 'Monitor of Built-in', 'Loopback', 'realtime-noise']) expect(isVirtualOrLoopbackAudioDevice(l)).toBe(true);
  expect(isVirtualOrLoopbackAudioDevice('Blue Yeti')).toBe(false);
});
it('refuses when only virtual devices exist and never calls the unconstrained getUserMedia', async () => {
  const gum = vi.fn();
  vi.stubGlobal('navigator', { mediaDevices: { enumerateDevices: async () => [{ kind: 'audioinput', label: 'ClearCore Virtual Mic', deviceId: 'v', groupId: 'g' }], getUserMedia: gum } });
  await expect(acquireRawPhysicalStream()).rejects.toBeInstanceOf(PhysicalMicUnavailableError);
  expect(gum.mock.calls.every(([c]) => c?.audio?.deviceId)).toBe(true);   // only exact-device requests, if any
});
it('opens the physical device with all processing off and returns a stable hash', async () => {
  const gum = vi.fn(async () => ({ getTracks: () => [] }) as unknown as MediaStream);
  vi.stubGlobal('navigator', { mediaDevices: { enumerateDevices: async () => [{ kind: 'audioinput', label: 'Blue Yeti', deviceId: 'abc', groupId: 'grp' }], getUserMedia: gum } });
  const a = await acquireRawPhysicalStream(); const b = await acquireRawPhysicalStream();
  expect(a.device.idHash).toMatch(/^[0-9a-f]{64}$/); expect(a.device.idHash).toBe(b.device.idHash);
  const c = gum.mock.calls.at(-1)![0].audio;
  expect([c.echoCancellation, c.noiseSuppression, c.autoGainControl]).toEqual([false, false, false]);
});
```
(`crypto.subtle` exists in Node 20+. The first `getUserMedia({audio:true})` label probe the old code did is allowed only if followed by `track.stop()`; the test above tolerates it.)
- [ ] **Step 2:** FAIL. **Step 3:** implement; delete nothing from `VoiceProfileCard.tsx` (E2 does). `PcmRecorder` is not unit-tested (needs `AudioContext`): keep its logic thin, and cover `concatChunks`/`measurePcm`. **Step 4:** PASS + `npm run build`. **Step 5: Commit** `feat(app): add raw 48 kHz PCM capture without fallbacks`.

### Task B1: Electron IPC module (`electron/enrollment-ipc.cjs`, `scripts/enrollment-ipc.selftest.cjs`)

**Interfaces (pure, `sendIpcRequest` injected):**
```js
const MAX_SPEECH_SECONDS = 90, RATE = 48000;
function buildAddVoiceSampleCommand({ pcm, sampleRate, name, device })
  // -> { AddVoiceSample: { name, pcm_f32_le_b64, sample_rate: 48000, device_label, device_id_hash } }
  // throws Error with .code 'ENROLL_INVALID_AUDIO' for: sampleRate != 48000, non-finite sample, empty, > 90 s, label not string,
  // idHash not /^[0-9a-f]{64}$/; label trimmed to 128 chars; name trimmed to 64.
function buildBuildProfileCommand({ name })     // -> { BuildVoiceProfile: { name } }
function buildGetJobCommand({ jobId })          // -> { GetEnrollmentJob: { job_id } } (jobId must match /^job-[0-9]+$/)
function mapJob(s)        // snake_case service JSON -> EnrollmentJob (types in enrollmentTypes.ts); unknown stage -> 'queued'
function mapSampleList(s) // -> { samples: ServiceSample[], budget: SpeechBudget }
function classifyEnrollError(err) // err.code matching /^ENROLL_[A-Z_]+$/ -> that code, else 'SERVICE_UNAVAILABLE' for connection errors, else 'ENROLL_FAILED'
function registerEnrollmentHandlers(ipcMain, { sendIpcRequest })
  // registers the five channels of CMD; timeouts: add_sample 60000 ms, others 5000 ms;
  // handlers return { errorCode } on failure and NEVER include the message text, pcm or the name in logs.
```
`pcm` arrives as a `Float32Array` or an `ArrayBuffer`/`Uint8Array` from the renderer; convert with `Buffer.from(f32.buffer, f32.byteOffset, f32.byteLength)` after asserting `os.endianness() === 'LE'`.

- [ ] **Step 1: Failing selftest** `scripts/enrollment-ipc.selftest.cjs` (plain `assert`, same style as `scripts/voice-profile-merge.selftest.cjs`):
```js
const assert = require('node:assert');
const m = require('../electron/enrollment-ipc.cjs');
const hash = 'a'.repeat(64);
const f = new Float32Array([0.1, -0.2, 0.3]);
const c = m.buildAddVoiceSampleCommand({ pcm: f, sampleRate: 48000, name: ' Ana ', device: { label: 'Mic', idHash: hash } });
assert.deepStrictEqual(Buffer.from(c.AddVoiceSample.pcm_f32_le_b64, 'base64'), Buffer.from(f.buffer));
assert.strictEqual(c.AddVoiceSample.name, 'Ana');
for (const bad of [{ sampleRate: 44100 }, { pcm: new Float32Array([NaN]) }, { pcm: new Float32Array(0) },
                   { pcm: new Float32Array(48000 * 91) }, { device: { label: 'x', idHash: 'zz' } }]) {
  assert.throws(() => m.buildAddVoiceSampleCommand({ pcm: f, sampleRate: 48000, name: 'n', device: { label: 'Mic', idHash: hash }, ...bad }),
                (e) => e.code === 'ENROLL_INVALID_AUDIO');
}
assert.deepStrictEqual(m.buildGetJobCommand({ jobId: 'job-12' }), { GetEnrollmentJob: { job_id: 'job-12' } });
assert.throws(() => m.buildGetJobCommand({ jobId: '../x' }));
assert.strictEqual(m.mapJob({ job_id: 'job-1', state: 'failed', stage: 'zzz', error_code: 'ENROLL_BUDGET_EXCEEDED', remaining_seconds: 2.5 }).stage, 'queued');
assert.strictEqual(m.mapJob({ job_id: 'job-1', state: 'failed', stage: 'trim', error_code: 'ENROLL_BUDGET_EXCEEDED', remaining_seconds: 2.5 }).remainingSeconds, 2.5);
assert.strictEqual(m.mapSampleList({ samples: [{ id: 's', name: 'n', timestamp: '1', speech_seconds: 4, device_label: 'M', used_in_profile: true, needs_reenroll: false, other_microphone: false }], budget: { used_seconds: 4, max_seconds: 90, remaining_seconds: 86 } }).budget.remainingSeconds, 86);
assert.strictEqual(m.classifyEnrollError(Object.assign(new Error('secret text'), { code: 'ENROLL_BUDGET_EXCEEDED' })), 'ENROLL_BUDGET_EXCEEDED');
assert.strictEqual(m.classifyEnrollError(new Error('connect ECONNREFUSED')), 'SERVICE_UNAVAILABLE');
// handlers: fake ipcMain collects channels; fake sendIpcRequest records (command, timeout)
const handlers = {}; const sent = [];
m.registerEnrollmentHandlers({ handle: (ch, fn) => { handlers[ch] = fn; } },
  { sendIpcRequest: async (cmd, payload, timeout) => { sent.push([cmd, timeout]); return { job_id: 'job-1' }; } });
assert.deepStrictEqual(Object.keys(handlers).sort(), ['enrollment_add_sample', 'enrollment_build_profile', 'enrollment_delete_sample', 'enrollment_get_job', 'enrollment_list_samples']);
(async () => {
  const r = await handlers.enrollment_add_sample({}, { pcm: f, sampleRate: 48000, name: 'n', device: { label: 'M', idHash: hash } });
  assert.deepStrictEqual(r, { jobId: 'job-1' }); assert.strictEqual(sent[0][1], 60000);
  const bad = await handlers.enrollment_add_sample({}, { pcm: f, sampleRate: 1, name: 'n', device: { label: 'M', idHash: hash } });
  assert.deepStrictEqual(bad, { errorCode: 'ENROLL_INVALID_AUDIO' });
  console.log('enrollment-ipc selftest passed.');
})();
```
- [ ] **Step 2:** `node scripts/enrollment-ipc.selftest.cjs` → FAIL (module missing). **Step 3:** implement. **Step 4:** PASS. **Step 5: Commit** `feat(app): add the pure Electron enrollment IPC module` (the `package.json` script chain is edited by E1, not here).

### Task C1: renderer client, budget rules, error mapping

**Interfaces:**
```ts
// speechBudget.ts (pure)
export function budgetPercent(b: SpeechBudget): number;                       // 0..100, clamped
export function wouldExceed(b: SpeechBudget, speechSeconds: number): boolean; // used + s > max (+1e-4 tolerance)
export function canRecordTake(b: SpeechBudget, takeSeconds: number): boolean; // remaining >= 5 && take <= remaining
export function formatSeconds(s: number): string;                             // "84,3" style via toFixed(1)
// enrollmentErrors.ts
export function enrollmentErrorCode(res: unknown): EnrollErrorCode | null;   // reads { errorCode } or a failed job's errorCode; unknown codes -> 'ENROLL_FAILED'
export function isBudgetError(code: EnrollErrorCode | null): boolean;
export function errorLabel(code: EnrollErrorCode | null, labels: EnrollmentLabels): string; // never returns free text from the service
// enrollmentClient.ts (invokeBridge from ../bridge)
export async function addSample(c: CapturedPcm, name: string): Promise<{ jobId: string } | { errorCode: EnrollErrorCode }>;
export async function buildProfile(name: string): Promise<{ jobId: string } | { errorCode: EnrollErrorCode }>;
export async function listSamples(): Promise<SampleList | { errorCode: EnrollErrorCode }>;
export async function deleteSample(id: string): Promise<boolean>;
export async function waitForJob(jobId: string, o?: { intervalMs?: number; timeoutMs?: number; onUpdate?: (j: EnrollmentJob) => void; getJob?: (id: string) => Promise<EnrollmentJob | { errorCode: EnrollErrorCode }> }): Promise<EnrollmentJob>;
```
`waitForJob` polls `CMD.getJob` every `intervalMs` (default 500) until `done`/`failed`, calls `onUpdate` each time, rejects with `Error('timeout')` after `timeoutMs` (default 120 000) — every poll doubles as the request that makes the service drain finished jobs.

- [ ] **Step 1: Failing tests** (`src/voice/__tests__/budget.test.ts`, `errors.test.ts`, `client.test.ts`):
```ts
const b = (used: number): SpeechBudget => ({ usedSeconds: used, maxSeconds: 90, remainingSeconds: Math.max(0, 90 - used) });
it('wouldExceed is exact at the boundary', () => { expect(wouldExceed(b(80), 10)).toBe(false); expect(wouldExceed(b(80), 10.01)).toBe(true); });
it('canRecordTake needs 5 s of margin and must fit', () => {
  expect(canRecordTake(b(86), 1)).toBe(false); expect(canRecordTake(b(80), 6)).toBe(true); expect(canRecordTake(b(80), 11)).toBe(false);
});
it('budgetPercent clamps', () => { expect(budgetPercent(b(45))).toBe(50); expect(budgetPercent(b(120))).toBe(100); });
it('unknown service codes never leak as text', () => {
  expect(enrollmentErrorCode({ errorCode: 'ENROLL_BUDGET_EXCEEDED' })).toBe('ENROLL_BUDGET_EXCEEDED');
  expect(enrollmentErrorCode({ errorCode: 'something <script>' })).toBe('ENROLL_FAILED');
  expect(enrollmentErrorCode({ ok: true })).toBeNull();
});
it('waitForJob polls until done and reports updates', async () => {
  const seq: EnrollmentJob[] = [job('running', 'denoise'), job('running', 'trim'), job('done', 'apply')];
  const seen: string[] = [];
  const out = await waitForJob('job-1', { intervalMs: 1, getJob: async () => seq.shift()!, onUpdate: (j) => seen.push(j.stage) });
  expect(out.state).toBe('done'); expect(seen).toEqual(['denoise', 'trim', 'apply']);
});
it('waitForJob stops on a failed job and exposes the budget error with remaining seconds', async () => {
  const out = await waitForJob('job-1', { intervalMs: 1, getJob: async () => ({ ...job('failed', 'trim'), errorCode: 'ENROLL_BUDGET_EXCEEDED', remainingSeconds: 2.5 }) });
  expect(out.errorCode).toBe('ENROLL_BUDGET_EXCEEDED'); expect(out.remainingSeconds).toBe(2.5);
});
it('waitForJob times out', async () => { await expect(waitForJob('job-1', { intervalMs: 1, timeoutMs: 5, getJob: async () => job('running', 'queued') })).rejects.toThrow('timeout'); });
```
(`job(state, stage)` is a local factory returning a full `EnrollmentJob`.)
- [ ] **Step 2–5:** FAIL → implement → PASS + `npm run build` → commit `feat(app): add the enrollment client, budget rules and error mapping`.

### Task D1: presentational components (props only, no bridge)

**Interfaces (all take `labels: EnrollmentLabels` and render plain HTML, no hooks that need a DOM):**
```tsx
export function VoiceBudgetMeter(p: { budget: SpeechBudget; labels: EnrollmentLabels }): JSX.Element;        // bar + "84,3 / 90 s"
export function VoiceSampleGallery(p: { samples: ServiceSample[]; budget: SpeechBudget; labels: EnrollmentLabels;
                                        onDelete: (id: string) => void; deletingId?: string | null; highlightDelete?: boolean }): JSX.Element;
export function BudgetErrorBanner(p: { remainingSeconds: number | null; labels: EnrollmentLabels }): JSX.Element; // delete prompt
export function EnrollmentJobStatus(p: { job: EnrollmentJob | null; labels: EnrollmentLabels }): JSX.Element | null;
export function DevModelNotice(p: { labels: EnrollmentLabels }): JSX.Element;
```
Gallery rows show: name, `speechSeconds` (one decimal), `deviceLabel`, a badge `otherMicrophone` / `needsReenroll` when set, `usedInProfile`/`notUsed`, and a delete button (`data-testid="delete-<id>"`), emphasised when `highlightDelete`.

- [ ] **Step 1: Failing tests** (`components.test.tsx`, `renderToStaticMarkup`):
```tsx
import { renderToStaticMarkup as html } from 'react-dom/server';
const labels = makeLabels();                                    // every key -> its own name, e.g. budgetTitle: 'budgetTitle'
it('meter shows used and max and clamps the bar at 100%', () => {
  const m = html(<VoiceBudgetMeter budget={{ usedSeconds: 120, maxSeconds: 90, remainingSeconds: 0 }} labels={labels} />);
  expect(m).toContain('90'); expect(m).toContain('width:100%');
});
it('gallery flags other-microphone and re-record samples and offers delete on each row', () => {
  const m = html(<VoiceSampleGallery labels={labels} budget={b0} onDelete={() => {}} samples={[
    { id: 'a', name: 'A', timestamp: '1', speechSeconds: 4.2, deviceLabel: 'Yeti', usedInProfile: true, needsReenroll: false, otherMicrophone: false },
    { id: 'b', name: 'B', timestamp: '2', speechSeconds: 0, deviceLabel: 'Old', usedInProfile: false, needsReenroll: true, otherMicrophone: true }]} />);
  expect(m).toContain('delete-a'); expect(m).toContain('delete-b'); expect(m).toContain('otherMicrophone'); expect(m).toContain('needsReenroll'); expect(m).toContain('4.2');
});
it('budget banner interpolates the remaining seconds and prompts to delete', () => {
  const m = html(<BudgetErrorBanner remainingSeconds={2.5} labels={{ ...labels, budgetExceededBody: 'Restam {remaining} s' }} />);
  expect(m).toContain('Restam 2.5 s');
});
it('job status maps stages to labels and shows the error label, never raw text', () => { /* running/trim -> stageTrim; failed/ENROLL_TOO_QUIET -> errors.ENROLL_TOO_QUIET */ });
it('dev model notice renders the label', () => { expect(html(<DevModelNotice labels={labels} />)).toContain('devModelNotice'); });
```
- [ ] **Step 2–5:** FAIL → implement (inline styles or the existing CSS classes of the card: `pill-active`/`pill-pending` are for status only; for the bar use inline `style={{ width: \`${pct}%\` }}`) → PASS + build → commit `feat(app): add the budget meter, sample gallery and job status components`.

---

## Wave 2 (parallel; after Wave 1 is cherry-picked)

### Task E1: Electron integration (`main.cjs` and friends)

**Changes:**
1. `main.cjs`: `require('./enrollment-ipc.cjs').registerEnrollmentHandlers(ipcMain, { sendIpcRequest })` once at startup (one line). Add an optional `timeoutMs` third parameter use already present.
2. Remove the obsolete handlers that fabricate data: `add_voice_sample`, `get_voice_samples`, `delete_voice_sample` (the renderer now uses the `enrollment_*` channels). `set_voice_profile` stops sending the local status as `profile_json`: it only forwards **`ClearVoiceProfile`** when `is_enrolled === false`; keep `buildSetVoiceProfileResult`.
3. `get_call_takes` / `approve_call_take` / `dismiss_call_take`: stop fabricating titles; pass through `speech_seconds`, `device_label`; `approve_call_take` returns `{ errorCode: 'ENROLL_BUDGET_EXCEEDED' }` when the service says so (use `classifyEnrollError`).
4. `voice-profile-store.cjs`: stop deriving `neural_eq_calibrated`/`is_enrolled` from the local sample count; the local file keeps only UI-neutral fields. Delete `writeVoiceSamples`, `addVoiceSample`, `deleteVoiceSample` and update `scripts/voice-profile-ipc.selftest.cjs` to the reduced store.
5. `bridge.ts`: remove the browser fallbacks that invent `embedding_dim: 192`, `neural_eq_calibrated`, `gain_boost_db: 1.8`; unknown `enrollment_*` commands pass to `window.clearcoreApi`'s generic invoke (no new mapping needed).
6. `package.json`: `test:electron-state` also runs `node scripts/enrollment-ipc.selftest.cjs && node scripts/voice-profile-ipc.selftest.cjs`.

- [ ] **Step 1:** extend `scripts/voice-profile-ipc.selftest.cjs` first (RED): assert that the store module no longer exports `addVoiceSample`/`deleteVoiceSample`/`writeVoiceSamples`, that reading a profile never returns `neural_eq_calibrated: true` for a profile with samples, and that `DEFAULT_PROFILE` has no `gain_boost_db`.
- [ ] **Step 2:** `node scripts/voice-profile-ipc.selftest.cjs` → FAIL.
- [ ] **Step 3:** make the changes above. `main.cjs` has no unit tests: keep each edit small, run `node --check electron/main.cjs`, and put any new logic in `enrollment-ipc.cjs` through a separate commit request to B1's owner only if truly needed (otherwise inline it with a comment).
- [ ] **Step 4:** `npm run build && npm test -- --run && npm run test:electron-state` → PASS (E2 runs concurrently; if a test fails only because it imports something E2 is changing, report it instead of editing E2's files).
- [ ] **Step 5: Commit** `feat(app): wire the enrollment channels and drop fabricated sample data in the Electron layer`.

### Task E2: renderer integration (`VoiceProfileCard.tsx`, types, i18n)

**Changes (in this order, commit after each):**
1. **Labels:** add the keys for every field of `EnrollmentLabels` (plus the card's new texts) to `src/i18n/types.ts`, `pt-BR.ts`, `en-US.ts`; add `buildEnrollmentLabels(t): EnrollmentLabels` (in `VoiceProfileCard.tsx` or a small exported helper there). Texts (pt-BR): `devModelNotice` = "Modelo de enrollment de desenvolvimento, ainda não aprovado"; `budgetExceededTitle` = "Limite de 90 s de fala atingido"; `budgetExceededBody` = "Restam {remaining} s. Apague algum áudio da galeria para adicionar este."; `otherMicrophone` = "Outro microfone (não usado)"; `needsReenroll` = "Regravar"; error labels for each `EnrollErrorCode` and `UNKNOWN`.
2. **Capture:** replace `MediaRecorder`, `createSyntheticAudioUrl`, the simulated VU, `encodeWav` and the unconstrained fallback by `acquireRawPhysicalStream` + `PcmRecorder`; if opening fails show the physical-microphone error (no recording starts). Keep `MIN/MAX_RECORDING_SECONDS` from `enrollmentTypes`. Re-export `isVirtualOrLoopbackAudioDevice`, `resolvePhysicalAudioDevice` from `captureDevice.ts` so existing tests still import them from the card.
3. **Samples:** guided step and voluntary modal call `addSample` then `waitForJob` (with `EnrollmentJobStatus`), show the returned quality, and on `ENROLL_BUDGET_EXCEEDED` show `BudgetErrorBanner` with the gallery open and `highlightDelete`. The gallery is `VoiceSampleGallery` fed by `listSamples()` (refresh after every add/delete/job). Remove `localStorage` sample/enrolled keys, the fabricated 5 samples (`durationSec: 5.0`), and `INITIAL_CALL_TAKES` mocks (takes list comes from the service only; empty list when none).
4. **Profile:** after the guided flow (or on the explicit "Refazer perfil" button) call `buildProfile(name)` + `waitForJob`; the status comes from the merged service status already implemented (Stage 1 helpers). Remove `neural_eq_calibrated: true` / `gain_boost_db: 1.8` / `embedding_dim: 192` constants from state and from the status fabricated for display; show `DevModelNotice` whenever the card is visible.
5. **Delete:** `handleDeleteSample` calls `deleteSample` then refreshes the list; it does **not** rebuild automatically (spec §4.4: only the user frees budget; rebuilding is the explicit button).

- [ ] **Step 1: RED:** add tests in `src/__tests__/voiceEnrollmentCard.test.ts` for the pure pieces you extract from the card (export them): `buildEnrollmentLabels` returns every key of `EnrollmentLabels` for both locales (no `undefined`); `nextStepAfterJob(job)` → `'done' | 'show-budget-error' | 'show-error'` (budget error carries `remainingSeconds`); `shouldOpenGalleryOnError(code)`; and update `voiceProfileAndStudioDsp.test.ts`/`voiceProfileIpc.test.ts` imports if you moved anything. Run → FAIL.
- [ ] **Step 2–3:** implement the five changes above, running `npm run build` after each (tsc catches every leftover reference).
- [ ] **Step 4:** `npm run build && npm test -- --run && npm run test:electron-state` → PASS. Grep proof (paste the output in the report): `grep -n "createSyntheticAudioUrl\|Math.random\|INITIAL_CALL_TAKES\|neural_eq_calibrated: true\|gain_boost_db: 1.8\|MediaRecorder\|localStorage" src/VoiceProfileCard.tsx` must be empty (or only the documented non-sample UI preferences).
- [ ] **Step 5: Commit** per change, e.g. `feat(app): record raw PCM and refuse virtual microphones`, `feat(app): show the speech budget, quality and delete prompt`, `feat(app): build the profile through the service job and label the development model`.

---

## Wave 3

### Task V2: verification (read-only) and the manual QA checklist
- [ ] Run: `npm run build`, `npm test -- --run`, `npm run test:electron-state`, `node scripts/voice-profile-ipc.selftest.cjs`, `node --check electron/main.cjs`; report counts.
- [ ] **Manual QA for the owner** (needs the service with `CLEARCORE_DEV_ENROLLMENT_ASSET` and `CLEARCORE_DEV_ENROLLMENT_SHA256` set, started by `./dev.sh`): (1) record 5 guided samples with the physical microphone — the card never offers the virtual one; (2) each sample shows peak, level and speech seconds; (3) "Refazer perfil" ends in "Aplicado no serviço (desenvolvimento)" with the development-model notice; (4) add samples until 90 s — the next manual sample shows the banner asking to delete audio, the gallery highlights delete, nothing was added; (5) plug a second microphone, record: the older samples show "Outro microfone (não usado)"; (6) kill the service while a build runs: the card shows the service error, never "ativo"; (7) check `~/.local/share/clearcore/profiles/` has only `samples/*.wav` (0600), `voice_samples.json`, `active_profile.json`, and no raw audio anywhere.
- [ ] Whole-branch review (Opus) against spec §6.1, §9, §12.

## Self-Review

- **Spec coverage:** D1 → A1, E2(2); D7 → A1 (hash), E2(3) gallery badges; D8/§4.4 → C1 (rules), D1 (banner, meter), E2(3,5); §6.1 → A1, E1, E2; §9 → D1 `DevModelNotice`, E2(4); §8 raw audio never persisted → B1 (no logs/disk), E2(3) (no localStorage); §5 client contract → W0 `CMD`, B1 mappings.
- **Dependencies on the service plan:** needs its Task 0 (command shapes) at minimum to implement B1's mappers; end-to-end QA needs its Wave 3.
- **Known uncertainties:** `AudioWorklet` loaded from a Blob URL under `file://` was not verified in Electron (A1 must try it in the real app via the `run` skill before declaring done; fallback: `ScriptProcessorNode`); `deviceId` stability across sessions can change the group hash (the hash combines `deviceId` and `groupId`; V2 QA step 5 exercises it); the renderer-to-main transfer of up to ~17 MB relies on Electron's structured clone.
