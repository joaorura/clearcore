'use strict';

const assert = require('assert');
const fs = require('fs');
const path = require('path');
const os = require('os');

const tempDir = path.join(os.tmpdir(), `clearcore-ipc-selftest-${Date.now()}`);
process.env.CLEARCORE_CONFIG_DIR = tempDir;

try {
  const store = require('../electron/voice-profile-store.cjs');

  console.log('Testing voice-profile-store default read...');
  const initial = store.readVoiceProfile();
  assert.strictEqual(initial.is_enrolled, false);
  assert.strictEqual(initial.active_samples_count, 0);
  assert.strictEqual(initial.embedding_dim, 192);

  console.log('Testing voice-profile-store write...');
  const written = store.writeVoiceProfile({
    is_enrolled: true,
    active_samples_count: 5,
    neural_eq_calibrated: true,
  });
  assert.strictEqual(written.is_enrolled, true);
  assert.strictEqual(written.active_samples_count, 5);

  console.log('Testing sample additions...');
  const sample = {
    id: 's-init-1',
    title: 'Início',
    category: 'Reunião',
    timestamp: '12:00',
    durationSec: 4.5,
  };
  const addRes = store.addVoiceSample(sample);
  assert.strictEqual(addRes.sample.id, 's-init-1');
  assert.strictEqual(addRes.samples.length, 1);

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

  const approveRes = store.approveCallTake('t-intake-1', 'Meet Validado', take);
  assert.strictEqual(approveRes.success, true);
  assert.strictEqual(approveRes.takes.length, 0);
  assert.strictEqual(approveRes.samples.length, 2);

  const profileAfterApprove = store.readVoiceProfile();
  assert.strictEqual(profileAfterApprove.is_enrolled, true);
  assert.strictEqual(profileAfterApprove.active_samples_count, 2);

  console.log('Testing sample deletion...');
  const delRes = store.deleteVoiceSample('s-init-1');
  assert.strictEqual(delRes.success, true);
  assert.strictEqual(delRes.samples.length, 1);

  console.log('All voice profile store self-tests passed successfully!');
} finally {
  delete process.env.CLEARCORE_CONFIG_DIR;
  if (fs.existsSync(tempDir)) {
    fs.rmSync(tempDir, { recursive: true, force: true });
  }
}
