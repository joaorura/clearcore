'use strict';
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
assert.deepStrictEqual(m.buildGetJobCommand({ jobId: 'profile-job-12' }), { GetEnrollmentJob: { job_id: 'profile-job-12' } });
assert.throws(() => m.buildGetJobCommand({ jobId: '../x' }));
// Job ids: the service prefixes the table (sample-job-N / profile-job-N); anything else is refused.
assert.deepStrictEqual(m.buildGetJobCommand({ jobId: 'sample-job-1' }), { GetEnrollmentJob: { job_id: 'sample-job-1' } });
for (const badId of ['job-1', 'sample-job-', 'profile-job-', 'sample-job-1;x', 'sample-job-1 ', 'other-job-1', '', 'sample-job-' + '9'.repeat(10), 'sample-job-' + '1'.repeat(400)]) {
  assert.throws(() => m.buildGetJobCommand({ jobId: badId }), undefined, badId);
  assert.strictEqual(m.mapJob({ job_id: badId, state: 'running' }).state, 'failed', badId);
}
assert.strictEqual(m.mapJob({ job_id: 'profile-job-12', state: 'running' }).jobId, 'profile-job-12');
assert.strictEqual(m.mapJob({ job_id: 'sample-job-1', state: 'failed', stage: 'zzz', error_code: 'ENROLL_BUDGET_EXCEEDED', remaining_seconds: 2.5 }).stage, 'queued');
assert.strictEqual(m.mapJob({ job_id: 'sample-job-1', state: 'failed', stage: 'trim', error_code: 'ENROLL_BUDGET_EXCEEDED', remaining_seconds: 2.5 }).remainingSeconds, 2.5);
assert.strictEqual(m.mapSampleList({ samples: [{ id: 's', name: 'n', timestamp: '1', speech_seconds: 4, device_label: 'M', used_in_profile: true, needs_reenroll: false, other_microphone: false }], budget: { used_seconds: 4, max_seconds: 90, remaining_seconds: 86 } }).budget.remainingSeconds, 86);
assert.strictEqual(m.classifyEnrollError(Object.assign(new Error('secret text'), { code: 'ENROLL_BUDGET_EXCEEDED' })), 'ENROLL_BUDGET_EXCEEDED');
assert.strictEqual(m.classifyEnrollError(Object.assign(new Error('connect'), { code: 'ECONNREFUSED' })), 'SERVICE_UNAVAILABLE');

// extras: pcm as ArrayBuffer / Uint8Array
const dev = { label: 'Mic', idHash: hash };
const fb = Buffer.from(f.buffer);
const ab = f.buffer.slice(0);
const viaAb = m.buildAddVoiceSampleCommand({ pcm: ab, sampleRate: 48000, name: 'n', device: dev });
assert.deepStrictEqual(Buffer.from(viaAb.AddVoiceSample.pcm_f32_le_b64, 'base64'), fb);
const viaU8 = m.buildAddVoiceSampleCommand({ pcm: new Uint8Array(ab), sampleRate: 48000, name: 'n', device: dev });
assert.deepStrictEqual(Buffer.from(viaU8.AddVoiceSample.pcm_f32_le_b64, 'base64'), fb);
// Uint8Array view with byteOffset
const big = new Uint8Array(ab.byteLength + 8); big.set(new Uint8Array(ab), 8);
const viaOff = m.buildAddVoiceSampleCommand({ pcm: big.subarray(8), sampleRate: 48000, name: 'n', device: dev });
assert.deepStrictEqual(Buffer.from(viaOff.AddVoiceSample.pcm_f32_le_b64, 'base64'), fb);
// truncation
assert.strictEqual(m.buildAddVoiceSampleCommand({ pcm: f, sampleRate: 48000, name: 'x'.repeat(100), device: dev }).AddVoiceSample.name.length, 64);
assert.strictEqual(m.buildAddVoiceSampleCommand({ pcm: f, sampleRate: 48000, name: 'n', device: { label: 'y'.repeat(300), idHash: hash } }).AddVoiceSample.device_label.length, 128);
assert.throws(() => m.buildAddVoiceSampleCommand({ pcm: f, sampleRate: 48000, name: 'n', device: { label: 5, idHash: hash } }), (e) => e.code === 'ENROLL_INVALID_AUDIO');
assert.throws(() => m.buildAddVoiceSampleCommand({ pcm: 'nope', sampleRate: 48000, name: 'n', device: dev }), (e) => e.code === 'ENROLL_INVALID_AUDIO');
assert.deepStrictEqual(m.buildBuildProfileCommand({ name: ' Ana ' }), { BuildVoiceProfile: { name: 'Ana' } });
// stage / job mapping
const j = m.mapJob({ job_id: 'sample-job-2', state: 'done', stage: 'apply', sample_id: 's1', profile_id: 'p1', quality: { peak: 0.5, rms_dbfs: -20, active_fraction: 0.7, speech_seconds: 3 } });
assert.deepStrictEqual(j.quality, { peak: 0.5, rmsDbfs: -20, activeFraction: 0.7, speechSeconds: 3 });
assert.strictEqual(j.sampleId, 's1'); assert.strictEqual(j.profileId, 'p1');
assert.strictEqual(j.errorCode, null); assert.strictEqual(j.remainingSeconds, null);
// errors
assert.strictEqual(m.classifyEnrollError(Object.assign(new Error('x'), { code: 'lowercase' })), 'ENROLL_FAILED');
assert.strictEqual(m.classifyEnrollError(new Error('Daemon unreachable at /x: connect ENOENT')), 'SERVICE_UNAVAILABLE');
assert.strictEqual(m.classifyEnrollError(new Error('service rejected: ENOENT')), 'ENROLL_FAILED');
assert.strictEqual(m.classifyEnrollError(Object.assign(new Error('x'), { code: 'ENROLL_WHATEVER' })), 'ENROLL_FAILED');
assert.strictEqual(m.classifyEnrollError(Object.assign(new Error('x'), { code: 'ENROLL_BUSY' })), 'ENROLL_BUSY');
// The active isolation model cannot apply a profile: its own code, on the closed list.
assert.strictEqual(m.classifyEnrollError(Object.assign(new Error('x'), { code: 'ENROLL_BACKEND_UNSUPPORTED' })), 'ENROLL_BACKEND_UNSUPPORTED');
assert.strictEqual(m.mapJob({ job_id: 'profile-job-2', state: 'failed', stage: 'apply', error_code: 'ENROLL_BACKEND_UNSUPPORTED' }).errorCode, 'ENROLL_BACKEND_UNSUPPORTED');
assert.strictEqual(m.classifyEnrollError(Object.assign(new Error('x'), { code: 'ETIMEDOUT' })), 'SERVICE_UNAVAILABLE');
assert.strictEqual(m.classifyEnrollError(new Error('Daemon IPC timeout')), 'SERVICE_UNAVAILABLE');
assert.strictEqual(m.classifyEnrollError(new Error('boom')), 'ENROLL_FAILED');

// handlers: fake ipcMain collects channels; fake sendIpcRequest records (command, timeout)
const handlers = {}; const sent = [];
let nextResult = { job_id: 'sample-job-1' };
let nextError = null;
m.registerEnrollmentHandlers({ handle: (ch, fn) => { handlers[ch] = fn; } },
  { sendIpcRequest: async (cmd, payload, timeout) => { sent.push([cmd, timeout]); if (nextError) throw nextError; return nextResult; } });
assert.deepStrictEqual(Object.keys(handlers).sort(), ['enrollment_add_sample', 'enrollment_build_profile', 'enrollment_delete_sample', 'enrollment_get_job', 'enrollment_list_samples']);
(async () => {
  const r = await handlers.enrollment_add_sample({}, { pcm: f, sampleRate: 48000, name: 'n', device: { label: 'M', idHash: hash } });
  assert.deepStrictEqual(r, { jobId: 'sample-job-1' }); assert.strictEqual(sent[0][1], 60000);
  const bad = await handlers.enrollment_add_sample({}, { pcm: f, sampleRate: 1, name: 'n', device: { label: 'M', idHash: hash } });
  assert.deepStrictEqual(bad, { errorCode: 'ENROLL_INVALID_AUDIO' });
  assert.strictEqual(sent.length, 1);

  await handlers.enrollment_build_profile({}, { name: 'Ana' });
  assert.deepStrictEqual(sent[1], [{ BuildVoiceProfile: { name: 'Ana' } }, 5000]);
  nextResult = { job_id: 'sample-job-3', state: 'running', stage: 'eq' };
  const g = await handlers.enrollment_get_job({}, { jobId: 'sample-job-3' });
  assert.strictEqual(g.stage, 'eq'); assert.strictEqual(sent[2][1], 5000);
  assert.deepStrictEqual(await handlers.enrollment_get_job({}, { jobId: '../x' }), { errorCode: 'ENROLL_FAILED' });
  nextResult = { samples: [], budget: { used_seconds: 0, max_seconds: 90, remaining_seconds: 90 } };
  const l = await handlers.enrollment_list_samples({}, {});
  assert.strictEqual(l.budget.maxSeconds, 90); assert.strictEqual(sent[3][0], 'ListVoiceSamples'); assert.strictEqual(sent[3][1], 5000);
  nextResult = {};
  assert.deepStrictEqual(await handlers.enrollment_delete_sample({}, { id: 's1' }), { success: true });
  assert.deepStrictEqual(sent[4], [{ DeleteVoiceSample: { id: 's1' } }, 5000]);

  // errors: classified code only, original message never leaks
  const logs = []; const origErr = console.error, origWarn = console.warn, origLog = console.log;
  console.error = console.warn = console.log = (...a) => logs.push(a.join(' '));
  try {
    nextError = Object.assign(new Error('SECRET-MSG pcm name Ana'), { code: 'ENROLL_BUDGET_EXCEEDED' });
    const e1 = await handlers.enrollment_add_sample({}, { pcm: f, sampleRate: 48000, name: 'Ana', device: dev });
    nextError = Object.assign(new Error('connect SECRET-MSG'), { code: 'ECONNREFUSED' });
    const e2 = await handlers.enrollment_list_samples({}, {});
    console.error = origErr; console.warn = origWarn; console.log = origLog;
    assert.deepStrictEqual(e1, { errorCode: 'ENROLL_BUDGET_EXCEEDED' });
    assert.deepStrictEqual(e2, { errorCode: 'SERVICE_UNAVAILABLE' });
    assert.ok(!JSON.stringify([e1, e2, logs]).includes('SECRET-MSG'));
    assert.ok(!logs.join('\n').includes('Ana'));
  } finally { console.error = origErr; console.warn = origWarn; console.log = origLog; }
  console.log('enrollment-ipc selftest passed.');
})().catch((e) => { console.error(e); process.exit(1); });

// --- round 1 fixes ---
{
  const mk = (name, label) => m.buildAddVoiceSampleCommand({ pcm: f, sampleRate: 48000, name, device: { label, idHash: hash } }).AddVoiceSample;
  const lone = (str) => /[\ud800-\udbff](?![\udc00-\udfff])|(?<![\ud800-\udbff])[\udc00-\udfff]/.test(str);
  const n1 = mk('x'.repeat(63) + '\u{1F600}', 'M').name;
  assert.ok(!lone(n1)); assert.strictEqual(n1, 'x'.repeat(63) + '\u{1F600}');
  const n1b = mk('x'.repeat(64) + '\u{1F600}', 'M').name;
  assert.ok(!lone(n1b)); assert.strictEqual(n1b, 'x'.repeat(64));
  const n2 = mk('x'.repeat(10) + '\ud83d' + 'y', 'M').name;
  assert.ok(!lone(n2)); assert.strictEqual(n2, 'x'.repeat(10) + 'y');
  const l1 = mk('n', '语'.repeat(128)).device_label;
  assert.ok(Buffer.byteLength(l1) <= 256); assert.strictEqual(l1, '语'.repeat(85));
  assert.ok(!lone(l1));
  const l2 = mk('n', '\u{1F600}'.repeat(100)).device_label;
  assert.ok(Buffer.byteLength(l2) <= 256); assert.strictEqual(Array.from(l2).length, 64); assert.ok(!lone(l2));
  const n3 = mk('语'.repeat(64), 'M').name;
  assert.ok(Buffer.byteLength(n3) <= 256); assert.strictEqual(Array.from(n3).length, 64);
  // fail closed
  for (const bad of [{}, { state: 'weird' }, { job_id: 'x', state: 'running' }, { job_id: 'sample-job-1' }, { job_id: 'sample-job-1', state: 'weird' }, null, undefined]) {
    const j = m.mapJob(bad);
    assert.strictEqual(j.state, 'failed'); assert.strictEqual(j.errorCode, 'ENROLL_FAILED');
  }
  assert.strictEqual(m.mapJob({ job_id: 'sample-job-1', state: 'running' }).state, 'running');
  assert.strictEqual(m.mapJob({ job_id: 'sample-job-1', state: 'failed', error_code: 'bogus' }).errorCode, 'ENROLL_FAILED');
  assert.strictEqual(m.mapJob({ job_id: 'sample-job-1', state: 'failed', remaining_seconds: Infinity }).remainingSeconds, null);
  assert.strictEqual(m.mapJob({ job_id: 'sample-job-1', state: 'failed', remaining_seconds: NaN }).remainingSeconds, null);
  // sample list filtering
  const sl = m.mapSampleList({ samples: [null, 5, {}, { id: 3 }, { id: 'ok', name: 'n' }], budget: {} });
  assert.deepStrictEqual(sl.samples.map((x) => x.id), ['ok']);
}
(async () => {
  const hs = {}; let res = { job_id: 'sample-job-9' };
  m.registerEnrollmentHandlers({ handle: (ch, fn) => { hs[ch] = fn; } }, { sendIpcRequest: async () => res });
  const dev = { label: 'M', idHash: hash };
  res = { job_id: 'oops' };
  assert.deepStrictEqual(await hs.enrollment_add_sample({}, { pcm: new Float32Array([0.5]), sampleRate: 48000, name: 'n', device: dev }), { errorCode: 'ENROLL_FAILED' });
  assert.deepStrictEqual(await hs.enrollment_build_profile({}, { name: 'n' }), { errorCode: 'ENROLL_FAILED' });
  res = {};
  assert.deepStrictEqual(await hs.enrollment_build_profile({}, { name: 'n' }), { errorCode: 'ENROLL_FAILED' });
  // PCM buffer zeroed after send (success and failure)
  res = { job_id: 'sample-job-9' };
  const p1 = new Float32Array([0.5, 0.25]);
  await hs.enrollment_add_sample({}, { pcm: p1, sampleRate: 48000, name: 'n', device: dev });
  assert.ok(p1.every((v) => v === 0));
  const p2 = new Uint8Array(new Float32Array([0.5]).buffer);
  await hs.enrollment_add_sample({}, { pcm: p2, sampleRate: 48000, name: 'n', device: dev });
  assert.ok(p2.every((v) => v === 0));
  const p3 = new Float32Array([0.5]);
  await hs.enrollment_add_sample({}, { pcm: p3, sampleRate: 1, name: 'n', device: dev });
  assert.ok(p3.every((v) => v === 0));
  res = { job_id: 'profile-job-4' };
  assert.deepStrictEqual(await hs.enrollment_build_profile({}, { name: 'n' }), { jobId: 'profile-job-4' });
  res = { job_id: 'job-4' };
  assert.deepStrictEqual(await hs.enrollment_build_profile({}, { name: 'n' }), { errorCode: 'ENROLL_FAILED' });
  assert.deepStrictEqual(await hs.enrollment_add_sample({}, { pcm: new Float32Array([0.5]), sampleRate: 48000, name: 'n', device: dev }), { errorCode: 'ENROLL_FAILED' });
  // Device group (spec §4.4, D7): passed through; absent or malformed -> null / '' (older service).
  const h1 = 'b'.repeat(64);
  const grp = m.mapSampleList({ selected_device_id_hash: h1, selected_device_label: 'Yeti', samples: [{ id: 'a', device_id_hash: h1 }, { id: 'b', device_id_hash: 'nothex' }, { id: 'c' }], budget: {} });
  assert.strictEqual(grp.selectedDeviceIdHash, h1);
  assert.strictEqual(grp.selectedDeviceLabel, 'Yeti');
  assert.deepStrictEqual(grp.samples.map((x) => x.deviceIdHash), [h1, null, null]);
  const old = m.mapSampleList({ samples: [], budget: {} });
  assert.strictEqual(old.selectedDeviceIdHash, null);
  assert.strictEqual(old.selectedDeviceLabel, '');
  const badGrp = m.mapSampleList({ selected_device_id_hash: 'X'.repeat(64), selected_device_label: 42, samples: [] });
  assert.strictEqual(badGrp.selectedDeviceIdHash, null);
  assert.strictEqual(badGrp.selectedDeviceLabel, '');
  // M6: the watchdog's timeout stage is kept (a failed job never becomes 'queued').
  const timedOut = m.mapJob({ job_id: 'profile-job-3', state: 'failed', stage: 'timeout', error_code: 'ENROLL_FAILED' });
  assert.strictEqual(timedOut.stage, 'timeout');
  assert.strictEqual(timedOut.state, 'failed');
  assert.strictEqual(timedOut.errorCode, 'ENROLL_FAILED');
  console.log('enrollment-ipc round-1 selftest passed.');
})().catch((e) => { console.error(e); process.exit(1); });

// --- I3: older daemon (no budget / parse errors) => SERVICE_OUTDATED, never "0 s remaining" ---
{
  const old = m.mapSampleList({ samples: [{ id: 's', name: 'n' }] });
  assert.strictEqual(old.serviceOutdated, true);
  assert.notStrictEqual(old.budget.remainingSeconds, 0);
  for (const b of [null, 'x', 5, []]) assert.strictEqual(m.mapSampleList({ samples: [], budget: b }).serviceOutdated, true);
  const cur = m.mapSampleList({ samples: [], budget: { used_seconds: 90, max_seconds: 90, remaining_seconds: 0 } });
  assert.strictEqual(cur.serviceOutdated, false);
  assert.strictEqual(cur.budget.remainingSeconds, 0);
}
(async () => {
  const hs = {}; let err = null;
  m.registerEnrollmentHandlers({ handle: (ch, fn) => { hs[ch] = fn; } }, { sendIpcRequest: async () => { if (err) throw err; return { job_id: 'sample-job-1' }; } });
  const dev = { label: 'M', idHash: hash };
  const add = () => hs.enrollment_add_sample({}, { pcm: new Float32Array([0.1]), sampleRate: 48000, name: 'n', device: dev });
  const build = () => hs.enrollment_build_profile({}, { name: 'n' });
  const outdated = [
    Object.assign(new Error('unknown field `pcm_f32_le_b64`, expected `sample_json`'), { code: 'JSON_PARSE_ERROR' }),
    Object.assign(new Error('malformed request'), { code: 'JSON_PARSE_ERROR' }),
    Object.assign(new Error('unknown variant `BuildVoiceProfile`'), { code: 'INVALID_COMMAND' }),
    new Error('unknown field `device_id_hash`'),
  ];
  for (const e of outdated) {
    err = e;
    assert.deepStrictEqual(await add(), { errorCode: 'SERVICE_OUTDATED' }, e.message);
    assert.deepStrictEqual(await build(), { errorCode: 'SERVICE_OUTDATED' }, e.message);
  }
  // A current service's own fixed refusals stay what they are.
  err = Object.assign(new Error('invalid sample metadata'), { code: 'INVALID_COMMAND' });
  assert.deepStrictEqual(await add(), { errorCode: 'ENROLL_FAILED' });
  err = Object.assign(new Error('invalid profile name'), { code: 'INVALID_COMMAND' });
  assert.deepStrictEqual(await build(), { errorCode: 'ENROLL_FAILED' });
  err = Object.assign(new Error('x'), { code: 'ENROLL_BUSY' });
  assert.deepStrictEqual(await build(), { errorCode: 'ENROLL_BUSY' });
  // Immediate refusal of BuildVoiceProfile by a backend without voice profile support.
  err = Object.assign(new Error('the active isolation model does not accept a voice profile'), { code: 'ENROLL_BACKEND_UNSUPPORTED' });
  assert.deepStrictEqual(await build(), { errorCode: 'ENROLL_BACKEND_UNSUPPORTED' });
  // Other channels keep the generic classification.
  err = Object.assign(new Error('malformed request'), { code: 'JSON_PARSE_ERROR' });
  assert.deepStrictEqual(await hs.enrollment_get_job({}, { jobId: 'sample-job-1' }), { errorCode: 'ENROLL_FAILED' });
  console.log('enrollment-ipc service-outdated selftest passed.');
})().catch((e) => { console.error(e); process.exit(1); });
