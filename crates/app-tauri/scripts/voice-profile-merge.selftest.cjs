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
const local = { is_enrolled: true, active_samples_count: 4, embedding_dim: 192, neural_eq_calibrated: false };

console.log('Testing offline/undefined/null service => no service keys...');
for (const s of [undefined, null, {}, 'x', 42, []]) {
  assert.deepStrictEqual(pickServiceVoiceProfileFields(s), {});
  const m = mergeLocalAndServiceProfile(local, s);
  assert.deepStrictEqual(m, local);
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
const stripped = stripServiceVoiceProfileFields({ ...local, ...Object.fromEntries(KEYS.map((k) => [k, 1])) });
assert.deepStrictEqual(stripped, local);
assert.deepStrictEqual(stripServiceVoiceProfileFields(null), {});

console.log('Testing buildSetVoiceProfileResult...');
const off = buildSetVoiceProfileResult({ updated: local, serviceStatus: {}, forwardError: 'service_unavailable' });
assert.strictEqual(off.voice_profile_error, 'service_unavailable');
for (const k of KEYS.filter((k) => k !== 'voice_profile_error')) assert.ok(!(k in off), `${k} absent`);
assert.strictEqual(off.is_enrolled, true);
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

console.log('voice-profile-merge selftest passed.');
