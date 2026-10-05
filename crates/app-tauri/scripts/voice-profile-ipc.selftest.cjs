'use strict';

const assert = require('assert');
const fs = require('fs');
const path = require('path');
const os = require('os');

const tempDir = path.join(os.tmpdir(), `clearcore-ipc-selftest-${Date.now()}`);
process.env.CLEARCORE_CONFIG_DIR = tempDir;

try {
  const store = require('../electron/voice-profile-store.cjs');

  console.log('Testing the store no longer fabricates or stores samples...');
  for (const name of ['addVoiceSample', 'deleteVoiceSample', 'writeVoiceSamples']) {
    assert.strictEqual(store[name], undefined, `${name} must not be exported`);
  }
  assert.strictEqual('gain_boost_db' in store.DEFAULT_PROFILE, false);
  assert.strictEqual('embedding_dim' in store.DEFAULT_PROFILE, false);
  assert.strictEqual('neural_eq_calibrated' in store.DEFAULT_PROFILE, false);

  console.log('Testing the legacy fabricated voice_samples.json is purged on first read...');
  const legacy = path.join(tempDir, 'voice_samples.json');
  fs.mkdirSync(tempDir, { recursive: true });
  fs.writeFileSync(legacy, '[{"id":"fake"}]');
  store.readVoiceProfile();
  assert.strictEqual(fs.existsSync(legacy), false);
  assert.strictEqual(store.readVoiceSamples, undefined);
  assert.strictEqual(store.getVoiceSamplesPath, undefined);

  console.log('Testing dead sample channels are gone from preload and bridge...');
  const dead = /getVoiceSamples|addVoiceSample|deleteVoiceSample|get_voice_samples|add_voice_sample|delete_voice_sample|clearcore_voice_sample_count/;
  for (const f of ['../electron/preload.cjs', '../src/bridge.ts']) {
    assert.ok(!dead.test(fs.readFileSync(path.join(__dirname, f), 'utf8')), `${f} still mentions a dead channel`);
  }

  console.log('Testing voice-profile-store default read...');
  const initial = store.readVoiceProfile();
  assert.strictEqual(initial.is_enrolled, false);
  assert.strictEqual(initial.active_samples_count, 0);
  assert.notStrictEqual(initial.neural_eq_calibrated, true);

  console.log('Testing voice-profile-store write...');
  const written = store.writeVoiceProfile({ is_enrolled: true, active_samples_count: 5 });
  assert.strictEqual(written.is_enrolled, true);
  assert.strictEqual(written.active_samples_count, 5);
  assert.notStrictEqual(store.readVoiceProfile().neural_eq_calibrated, true);

  console.log('Testing call takes intake...');
  const take = {
    id: 't-intake-1',
    title: 'Meet Call',
    timestamp: '12:05',
    durationSec: 5.5,
    snrDb: 25.0,
  };
  store.writeCallTakes([take]);
  assert.strictEqual(store.readCallTakes().length, 1);

  const approveRes = store.approveCallTake('t-intake-1');
  assert.strictEqual(approveRes.success, true);
  assert.strictEqual(approveRes.takes.length, 0);
  assert.strictEqual('samples' in approveRes, false);

  const profileAfterApprove = store.readVoiceProfile();
  assert.strictEqual(profileAfterApprove.active_samples_count, 5);
  assert.notStrictEqual(profileAfterApprove.neural_eq_calibrated, true);

  console.log('All voice profile store self-tests passed successfully!');
} finally {
  delete process.env.CLEARCORE_CONFIG_DIR;
  if (fs.existsSync(tempDir)) {
    fs.rmSync(tempDir, { recursive: true, force: true });
  }
}
