'use strict';

// Self-test for electron/clearcore-state.cjs. Plain node, no test runner:
//   node scripts/clearcore-state.selftest.cjs
const assert = require('node:assert/strict');
const fs = require('fs');
const os = require('os');
const path = require('path');

const state = require('../electron/clearcore-state.cjs');

function bytes(...values) {
  const buf = Buffer.alloc(16, 0);
  values.forEach(([offset, value]) => buf.writeUInt32LE(value, offset));
  return buf;
}

const tests = [];
function test(name, fn) {
  tests.push([name, fn]);
}

test('encodes the mode at offset 0 and keeps the rest zero', () => {
  assert.deepEqual(state.encodeState(undefined, { mode: 'Bypass' }), bytes([0, 1]));
  assert.deepEqual(state.encodeState(undefined, { mode: 'Mute' }), bytes([0, 2]));
  assert.deepEqual(state.encodeState(undefined, { mode: 'Active' }), bytes());
});

test('updating the mode preserves target node, generation and preset', () => {
  const existing = bytes([4, 7], [8, 5], [12, 2]);
  assert.deepEqual(
    state.encodeState(existing, { mode: 'Mute' }),
    bytes([0, 2], [4, 7], [8, 5], [12, 2])
  );
});

test('updating the preset preserves the mode', () => {
  const existing = bytes([0, 1], [4, 7]);
  assert.deepEqual(
    state.encodeState(existing, { preset: 'Broadcast' }),
    bytes([0, 1], [4, 7], [12, 3])
  );
});

test('an unknown preset name is ignored, not turned into Off', () => {
  const existing = bytes([12, 2]);
  assert.deepEqual(state.encodeState(existing, { preset: 'Loud' }), existing);
  assert.deepEqual(state.encodeState(existing, { preset: 2 }), existing);
});

test('target node and generation are written as 32-bit values and never reach the preset', () => {
  const existing = bytes([12, 3]);
  const out = state.encodeState(existing, { targetNodeId: '42', generation: 0xffffffff });
  assert.deepEqual(out, bytes([4, 42], [8, 0xffffffff], [12, 3]));
});

test('only the first 16 bytes of a longer file are used; a shorter one is ignored', () => {
  const longer = Buffer.concat([bytes([12, 1]), Buffer.alloc(8, 0xaa)]);
  assert.deepEqual(state.encodeState(longer, { mode: 'Bypass' }), bytes([0, 1], [12, 1]));
  assert.deepEqual(state.encodeState(Buffer.alloc(8, 0xff), { mode: 'Bypass' }), bytes([0, 1]));
});

test('readPreset returns the name, null for a missing file and null for an invalid value', () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'clearcore-state-'));
  try {
    const file = path.join(dir, 'clearcore_state');
    assert.equal(state.readPreset(file), null);
    fs.writeFileSync(file, bytes([12, 2]));
    assert.equal(state.readPreset(file), 'Podcast');
    fs.writeFileSync(file, bytes([12, 9]));
    assert.equal(state.readPreset(file), null);
    fs.writeFileSync(file, Buffer.alloc(8));
    assert.equal(state.readPreset(file), null);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test('updateStateFile creates a 16-byte file and never truncates an existing one', () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'clearcore-state-'));
  try {
    const file = path.join(dir, 'clearcore_state');
    state.updateStateFile(file, { mode: 'Bypass', preset: 'Natural' });
    assert.equal(fs.statSync(file).size, 16);
    assert.deepEqual(fs.readFileSync(file), bytes([0, 1], [12, 1]));

    // A file that is longer than the contract keeps its size; an in-place write never shrinks it.
    fs.writeFileSync(file, Buffer.concat([bytes([0, 2]), Buffer.alloc(8, 0xaa)]));
    state.updateStateFile(file, { preset: 'Broadcast' });
    const after = fs.readFileSync(file);
    assert.equal(after.length, 24);
    assert.deepEqual(after.subarray(0, 16), bytes([0, 2], [12, 3]));

    // A short file grows to 16 bytes (never the other way around).
    fs.writeFileSync(file, Buffer.alloc(4, 0));
    state.updateStateFile(file, { mode: 'Mute' });
    assert.equal(fs.statSync(file).size, 16);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test('the state file is created owner-only (0600) even with a permissive umask', () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'clearcore-state-'));
  const oldUmask = process.umask(0);
  try {
    const file = path.join(dir, 'clearcore_state');
    state.updateStateFile(file, { mode: 'Active' });
    assert.equal(fs.statSync(file).mode & 0o777, 0o600);
  } finally {
    process.umask(oldUmask);
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test('the mapped size seen by a reader is stable across writes', () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'clearcore-state-'));
  try {
    const file = path.join(dir, 'clearcore_state');
    state.updateStateFile(file, { mode: 'Active' });
    const fd = fs.openSync(file, 'r');
    try {
      for (const preset of state.PRESET_NAMES) {
        state.updateStateFile(file, { preset });
        assert.equal(fs.fstatSync(fd).size, 16, `size changed while writing ${preset}`);
      }
    } finally {
      fs.closeSync(fd);
    }
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test('a preset update writes only its own 4 bytes, so a concurrent mode change survives', () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'clearcore-state-'));
  const realWriteSync = fs.writeSync;
  try {
    const file = path.join(dir, 'clearcore_state');
    state.updateStateFile(file, { mode: 'Active', targetNodeId: 7 });

    // The helper flips the mode to Mute right before our write lands (the window between any
    // read-modify-write cycle). A full 16-byte rewrite would put the stale Active back.
    const writes = [];
    let raced = false;
    fs.writeSync = (fd, buffer, ...rest) => {
      if (!raced) {
        raced = true;
        const other = fs.openSync(file, 'r+');
        try {
          const mute = Buffer.alloc(4);
          mute.writeUInt32LE(2, 0);
          realWriteSync(other, mute, 0, 4, 0);
        } finally {
          fs.closeSync(other);
        }
      }
      writes.push({ length: buffer.length, rest });
      return realWriteSync(fd, buffer, ...rest);
    };
    try {
      state.updateStateFile(file, { preset: 'Podcast' });
    } finally {
      fs.writeSync = realWriteSync;
    }

    assert.deepEqual(fs.readFileSync(file), bytes([0, 2], [4, 7], [12, 2]));
    assert.equal(writes.length, 1, 'one field changed, so exactly one write');
    assert.equal(writes[0].length, 4, 'the write must cover only the 4-byte field');
  } finally {
    fs.writeSync = realWriteSync;
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test('a mode update leaves the preset and target node bytes alone', () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'clearcore-state-'));
  try {
    const file = path.join(dir, 'clearcore_state');
    fs.writeFileSync(file, bytes([4, 9], [8, 3], [12, 3]));
    state.updateStateFile(file, { mode: 'Bypass' });
    assert.deepEqual(fs.readFileSync(file), bytes([0, 1], [4, 9], [8, 3], [12, 3]));
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test('a symlink at the state path is refused and its target is never touched', () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'clearcore-state-'));
  try {
    const victim = path.join(dir, 'victim');
    fs.writeFileSync(victim, 'precious');
    const file = path.join(dir, 'clearcore_state');
    fs.symlinkSync(victim, file);
    assert.throws(() => state.updateStateFile(file, { preset: 'Natural' }));
    assert.equal(fs.readFileSync(victim, 'utf8'), 'precious');
    assert.equal(state.readPreset(file), null);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test('a non-regular file at the state path is refused', () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'clearcore-state-'));
  try {
    const file = path.join(dir, 'clearcore_state');
    fs.mkdirSync(file);
    assert.throws(() => state.updateStateFile(file, { preset: 'Natural' }));
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

let failed = 0;
for (const [name, fn] of tests) {
  try {
    fn();
    console.log(`ok - ${name}`);
  } catch (error) {
    failed += 1;
    console.error(`not ok - ${name}\n${error.stack}`);
  }
}
console.log(`${tests.length - failed}/${tests.length} passed`);
process.exit(failed === 0 ? 0 : 1);
