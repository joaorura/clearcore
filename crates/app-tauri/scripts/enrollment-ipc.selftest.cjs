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
assert.deepStrictEqual(m.buildGetJobCommand({ jobId: 'job-12' }), { GetEnrollmentJob: { job_id: 'job-12' } });
assert.throws(() => m.buildGetJobCommand({ jobId: '../x' }));
assert.strictEqual(m.mapJob({ job_id: 'job-1', state: 'failed', stage: 'zzz', error_code: 'ENROLL_BUDGET_EXCEEDED', remaining_seconds: 2.5 }).stage, 'queued');
assert.strictEqual(m.mapJob({ job_id: 'job-1', state: 'failed', stage: 'trim', error_code: 'ENROLL_BUDGET_EXCEEDED', remaining_seconds: 2.5 }).remainingSeconds, 2.5);
assert.strictEqual(m.mapSampleList({ samples: [{ id: 's', name: 'n', timestamp: '1', speech_seconds: 4, device_label: 'M', used_in_profile: true, needs_reenroll: false, other_microphone: false }], budget: { used_seconds: 4, max_seconds: 90, remaining_seconds: 86 } }).budget.remainingSeconds, 86);
assert.strictEqual(m.classifyEnrollError(Object.assign(new Error('secret text'), { code: 'ENROLL_BUDGET_EXCEEDED' })), 'ENROLL_BUDGET_EXCEEDED');
assert.strictEqual(m.classifyEnrollError(new Error('connect ECONNREFUSED')), 'SERVICE_UNAVAILABLE');

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
const j = m.mapJob({ job_id: 'job-2', state: 'done', stage: 'apply', sample_id: 's1', profile_id: 'p1', quality: { peak: 0.5, rms_dbfs: -20, active_fraction: 0.7, speech_seconds: 3 } });
assert.deepStrictEqual(j.quality, { peak: 0.5, rmsDbfs: -20, activeFraction: 0.7, speechSeconds: 3 });
assert.strictEqual(j.sampleId, 's1'); assert.strictEqual(j.profileId, 'p1');
assert.strictEqual(j.errorCode, null); assert.strictEqual(j.remainingSeconds, null);
// errors
assert.strictEqual(m.classifyEnrollError(Object.assign(new Error('x'), { code: 'lowercase' })), 'ENROLL_FAILED');
assert.strictEqual(m.classifyEnrollError(new Error('Daemon unreachable at /x: connect ENOENT')), 'SERVICE_UNAVAILABLE');
assert.strictEqual(m.classifyEnrollError(new Error('Daemon IPC timeout')), 'SERVICE_UNAVAILABLE');
assert.strictEqual(m.classifyEnrollError(new Error('boom')), 'ENROLL_FAILED');

// handlers: fake ipcMain collects channels; fake sendIpcRequest records (command, timeout)
const handlers = {}; const sent = [];
let nextResult = { job_id: 'job-1' };
let nextError = null;
m.registerEnrollmentHandlers({ handle: (ch, fn) => { handlers[ch] = fn; } },
  { sendIpcRequest: async (cmd, payload, timeout) => { sent.push([cmd, timeout]); if (nextError) throw nextError; return nextResult; } });
assert.deepStrictEqual(Object.keys(handlers).sort(), ['enrollment_add_sample', 'enrollment_build_profile', 'enrollment_delete_sample', 'enrollment_get_job', 'enrollment_list_samples']);
(async () => {
  const r = await handlers.enrollment_add_sample({}, { pcm: f, sampleRate: 48000, name: 'n', device: { label: 'M', idHash: hash } });
  assert.deepStrictEqual(r, { jobId: 'job-1' }); assert.strictEqual(sent[0][1], 60000);
  const bad = await handlers.enrollment_add_sample({}, { pcm: f, sampleRate: 1, name: 'n', device: { label: 'M', idHash: hash } });
  assert.deepStrictEqual(bad, { errorCode: 'ENROLL_INVALID_AUDIO' });
  assert.strictEqual(sent.length, 1);

  await handlers.enrollment_build_profile({}, { name: 'Ana' });
  assert.deepStrictEqual(sent[1], [{ BuildVoiceProfile: { name: 'Ana' } }, 5000]);
  nextResult = { job_id: 'job-3', state: 'running', stage: 'eq' };
  const g = await handlers.enrollment_get_job({}, { jobId: 'job-3' });
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
    nextError = new Error('connect ECONNREFUSED SECRET-MSG');
    const e2 = await handlers.enrollment_list_samples({}, {});
    console.error = origErr; console.warn = origWarn; console.log = origLog;
    assert.deepStrictEqual(e1, { errorCode: 'ENROLL_BUDGET_EXCEEDED' });
    assert.deepStrictEqual(e2, { errorCode: 'SERVICE_UNAVAILABLE' });
    assert.ok(!JSON.stringify([e1, e2, logs]).includes('SECRET-MSG'));
    assert.ok(!logs.join('\n').includes('Ana'));
  } finally { console.error = origErr; console.warn = origWarn; console.log = origLog; }
  console.log('enrollment-ipc selftest passed.');
})().catch((e) => { console.error(e); process.exit(1); });
