'use strict';

const assert = require('assert');
const {
  pickServiceVoiceProfileFields,
  mergeLocalAndServiceProfile,
  stripServiceVoiceProfileFields,
  buildSetVoiceProfileResult,
  classifyForwardError,
} = require('../electron/voice-profile-merge.cjs');

const KEYS = [
  'is_voice_profile_active',
  'stored_voice_profile_id',
  'voice_profile_error',
  'voice_profile_selected',
  'active_voice_profile_id',
];
const local = { is_enrolled: true, active_samples_count: 4, embedding_dim: 192 };

console.log('Testing offline/undefined/null service => no service keys...');
for (const s of [undefined, null, {}, 'x', 42, []]) {
  assert.deepStrictEqual(pickServiceVoiceProfileFields(s), {});
  const m = mergeLocalAndServiceProfile(local, s);
  // The local is_enrolled is ignored: without a service status no profile exists.
  assert.deepStrictEqual(m, { ...local, is_enrolled: false });
  for (const k of KEYS) assert.ok(!(k in m), `${k} must be absent`);
}

console.log('Testing partial service status...');
const partial = pickServiceVoiceProfileFields({ stored_voice_profile_id: 'abc', unrelated: 1 });
assert.deepStrictEqual(partial, { stored_voice_profile_id: 'abc' });

console.log('Testing false stays false, absent stays absent...');
assert.strictEqual(pickServiceVoiceProfileFields({ is_voice_profile_active: false }).is_voice_profile_active, false);
assert.ok(!('is_voice_profile_active' in pickServiceVoiceProfileFields({ stored_voice_profile_id: 'abc' })));

console.log('Testing non-boolean active ignored...');
for (const v of ['true', 1, null, {}]) {
  assert.ok(!('is_voice_profile_active' in pickServiceVoiceProfileFields({ is_voice_profile_active: v })));
}

console.log('Testing local preserved and service overrides...');
const merged = mergeLocalAndServiceProfile(
  { ...local, voice_profile_error: 'old', stored_voice_profile_id: 'old' },
  { voice_profile_error: null, stored_voice_profile_id: 'new', is_voice_profile_active: true },
);
assert.strictEqual(merged.is_enrolled, true);
assert.strictEqual(merged.active_samples_count, 4);
assert.strictEqual(merged.stored_voice_profile_id, 'new');
assert.strictEqual(merged.voice_profile_error, null);
assert.strictEqual(merged.is_voice_profile_active, true);

console.log('Testing strip service fields...');
const stripped = stripServiceVoiceProfileFields({ ...local, has_voice_profile: true, ...Object.fromEntries(KEYS.map((k) => [k, 1])) });
assert.deepStrictEqual(stripped, local);
assert.deepStrictEqual(stripServiceVoiceProfileFields(null), {});

console.log('Testing buildSetVoiceProfileResult...');
const off = buildSetVoiceProfileResult({ updated: local, serviceStatus: {}, forwardError: 'service_unavailable' });
assert.strictEqual(off.voice_profile_error, 'service_unavailable');
for (const k of KEYS.filter((k) => k !== 'voice_profile_error')) assert.ok(!(k in off), `${k} absent`);
assert.strictEqual(off.is_enrolled, false);
const on = buildSetVoiceProfileResult({
  updated: local,
  serviceStatus: { is_voice_profile_active: true, stored_voice_profile_id: 'p1' },
  forwardError: null,
});
assert.strictEqual(on.is_voice_profile_active, true);
assert.strictEqual(on.stored_voice_profile_id, 'p1');
assert.ok(!('voice_profile_error' in on));
const both = buildSetVoiceProfileResult({ updated: local, serviceStatus: { voice_profile_error: 'svc' }, forwardError: 'service_rejected' });
assert.strictEqual(both.voice_profile_error, 'service_rejected');

console.log('Testing classifyForwardError never leaks JSON...');
const leak = '{"profile_json":"{secret}"}';
assert.strictEqual(classifyForwardError(new Error('Daemon unreachable at x: ' + leak)), 'service_unavailable');
assert.strictEqual(classifyForwardError(new Error('Daemon IPC timeout')), 'service_unavailable');
assert.strictEqual(classifyForwardError(new Error('Failed to parse daemon response: ' + leak)), 'service_error');
assert.strictEqual(classifyForwardError(Object.assign(new Error(leak), { code: 'INVALID_PROFILE' })), 'INVALID_PROFILE');
assert.strictEqual(classifyForwardError(Object.assign(new Error(leak), { code: 'bad{code' })), 'service_rejected');
assert.strictEqual(classifyForwardError(Object.assign(new Error(leak), { serviceRejected: true })), 'service_rejected');
for (const e of [new Error(leak), null, undefined, 'x']) {
  const c = classifyForwardError(e);
  assert.ok(!c.includes('{') && c.length <= 64);
}

console.log('Testing is_enrolled comes from the service, never from the local file...');
// Legacy flow: the local file says enrolled but the service has no profile.
const legacy = mergeLocalAndServiceProfile(
  { is_enrolled: true, active_samples_count: 3 },
  { stored_voice_profile_id: null, has_voice_profile: false, is_voice_profile_active: false },
);
assert.strictEqual(legacy.is_enrolled, false);
assert.strictEqual(legacy.has_voice_profile, false);
// New flow: the local file never says enrolled, the service holds a profile.
assert.strictEqual(mergeLocalAndServiceProfile({ is_enrolled: false }, { stored_voice_profile_id: 'p1' }).is_enrolled, true);
assert.strictEqual(mergeLocalAndServiceProfile({}, { has_voice_profile: true }).is_enrolled, true);
assert.strictEqual(mergeLocalAndServiceProfile({ is_enrolled: true }, { stored_voice_profile_id: '' }).is_enrolled, false);
assert.strictEqual(mergeLocalAndServiceProfile({ is_enrolled: true }, { has_voice_profile: 'yes' }).is_enrolled, false);

console.log('Testing voice_profile_supported and neural_eq_calibrated come only from the service...');
// A local file from the old flow that claims calibration is ignored, online or offline.
const legacyEq = { is_enrolled: true, active_samples_count: 1, neural_eq_calibrated: true };
assert.ok(!('neural_eq_calibrated' in mergeLocalAndServiceProfile(legacyEq, {})));
assert.ok(!('neural_eq_calibrated' in mergeLocalAndServiceProfile(legacyEq, { stored_voice_profile_id: 'p1' })));
assert.strictEqual(mergeLocalAndServiceProfile(legacyEq, { neural_eq_calibrated: false }).neural_eq_calibrated, false);
assert.strictEqual(mergeLocalAndServiceProfile({}, { neural_eq_calibrated: true }).neural_eq_calibrated, true);
assert.strictEqual(mergeLocalAndServiceProfile({}, { voice_profile_supported: false }).voice_profile_supported, false);
assert.strictEqual(mergeLocalAndServiceProfile({}, { voice_profile_supported: true }).voice_profile_supported, true);
for (const v of ['false', 0, null, {}]) {
  const m = mergeLocalAndServiceProfile({}, { voice_profile_supported: v, neural_eq_calibrated: v });
  assert.ok(!('voice_profile_supported' in m), 'non-boolean support = unknown');
  assert.ok(!('neural_eq_calibrated' in m), 'non-boolean calibration dropped');
}
// Absent (older service): unknown, never invented.
assert.ok(!('voice_profile_supported' in mergeLocalAndServiceProfile({}, { stored_voice_profile_id: 'p1' })));
assert.deepStrictEqual(stripServiceVoiceProfileFields({ a: 1, voice_profile_supported: false, neural_eq_calibrated: true }), { a: 1 });

console.log('Testing dev_base_model / dev_base_model_error are validated service fields...');
assert.strictEqual(mergeLocalAndServiceProfile({}, { dev_base_model: 'pdfnet3-dev' }).dev_base_model, 'pdfnet3-dev');
assert.strictEqual(mergeLocalAndServiceProfile({}, { dev_base_model: 'base' }).dev_base_model, 'base');
for (const v of ['approved', true, null, 7]) {
  assert.ok(!('dev_base_model' in mergeLocalAndServiceProfile({}, { dev_base_model: v })), 'unknown model dropped');
}
assert.strictEqual(mergeLocalAndServiceProfile({}, { dev_base_model_error: 'DEV_MODEL_NO_FILM' }).dev_base_model_error, 'DEV_MODEL_NO_FILM');
assert.strictEqual(mergeLocalAndServiceProfile({}, { dev_base_model_error: null }).dev_base_model_error, null);
for (const v of ['/home/u/pdfnet3.tar.gz', 'boom', 1]) {
  assert.ok(!('dev_base_model_error' in mergeLocalAndServiceProfile({}, { dev_base_model_error: v })), 'free text dropped');
}
// Never read from the local file.
assert.ok(!('dev_base_model' in mergeLocalAndServiceProfile({ dev_base_model: 'pdfnet3-dev' }, {})));

console.log('voice-profile-merge selftest passed.');
