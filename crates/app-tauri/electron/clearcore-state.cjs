'use strict';

// Layout of $XDG_RUNTIME_DIR/clearcore_state, shared with the native PipeWire helper
// (platform/linux/helper/src/clearcore_state.h). 16 bytes, little-endian, never changes size:
//   0  u32 mode            0=Active, 1=Bypass, 2=Mute
//   4  u32 target_node_id  physical microphone node (0=auto)
//   8  u32 generation      unused by the helper
//   12 u32 preset          0=Off, 1=Natural, 2=Podcast, 3=Broadcast (studio finishing)
// The Electron app is the only writer of `preset`.

const fs = require('fs');

const STATE_SIZE = 16;
const OFFSET_MODE = 0;
const OFFSET_TARGET_NODE_ID = 4;
const OFFSET_GENERATION = 8;
const OFFSET_PRESET = 12;

const PRESET_VALUES = Object.freeze({ Off: 0, Natural: 1, Podcast: 2, Broadcast: 3 });
const PRESET_NAMES = Object.freeze(['Off', 'Natural', 'Podcast', 'Broadcast']);

function isPreset(value) {
  return typeof value === 'string' && Object.prototype.hasOwnProperty.call(PRESET_VALUES, value);
}

// The 4-byte fields that `updates` changes, as [offset, value] pairs. Fields absent from `updates`
// (and an unknown preset name) produce nothing, so they are never written.
function fieldWrites(updates = {}) {
  const writes = [];
  if (updates.mode !== undefined) {
    const modeVal = updates.mode === 'Bypass' ? 1 : updates.mode === 'Mute' ? 2 : 0;
    writes.push([OFFSET_MODE, modeVal]);
  }
  if (updates.targetNodeId !== undefined) {
    writes.push([OFFSET_TARGET_NODE_ID, Number(updates.targetNodeId) >>> 0]);
  }
  if (updates.generation !== undefined) {
    writes.push([OFFSET_GENERATION, Number(updates.generation) >>> 0]);
  }
  if (updates.preset !== undefined && isPreset(updates.preset)) {
    writes.push([OFFSET_PRESET, PRESET_VALUES[updates.preset]]);
  }
  return writes;
}

// Builds the 16-byte buffer: starts from the first 16 bytes of `existing` (when it has at least
// 16) and applies only the fields present in `updates`, so everything else is preserved.
// Pure helper (used by the self-test); updateStateFile does NOT write this whole buffer.
function encodeState(existing, updates = {}) {
  const buf = Buffer.alloc(STATE_SIZE, 0);
  if (Buffer.isBuffer(existing) && existing.length >= STATE_SIZE) {
    existing.copy(buf, 0, 0, STATE_SIZE);
  }
  for (const [offset, value] of fieldWrites(updates)) {
    buf.writeUInt32LE(value, offset);
  }
  return buf;
}

// Opens the state file without following a symlink in the final path component (the file lives in
// a shared directory when XDG_RUNTIME_DIR is unset) and checks, on the open descriptor, that it is
// a regular file owned by this user. Returns the fd; throws (and closes it) otherwise.
function openStateFile(statePath, flags) {
  const noFollow = fs.constants.O_NOFOLLOW || 0;
  const cloexec = fs.constants.O_CLOEXEC || 0;
  const fd = fs.openSync(statePath, flags | noFollow | cloexec, 0o600);
  try {
    const st = fs.fstatSync(fd);
    if (!st.isFile()) {
      throw new Error(`${statePath} is not a regular file`);
    }
    if (typeof process.getuid === 'function' && st.uid !== process.getuid()) {
      throw new Error(`${statePath} is not owned by the current user`);
    }
  } catch (error) {
    fs.closeSync(fd);
    throw error;
  }
  return fd;
}

// Preset stored in the file: a name, or null if the file is missing, short, not a safe regular
// file, or holds a value outside 0..3 (the helper ignores such values too).
function readPreset(statePath) {
  let fd;
  try {
    fd = openStateFile(statePath, fs.constants.O_RDONLY);
  } catch {
    return null;
  }
  try {
    const buf = Buffer.alloc(STATE_SIZE, 0);
    const read = fs.readSync(fd, buf, 0, STATE_SIZE, 0);
    if (read < STATE_SIZE) return null;
    const raw = buf.readUInt32LE(OFFSET_PRESET);
    return raw < PRESET_NAMES.length ? PRESET_NAMES[raw] : null;
  } catch {
    return null;
  } finally {
    fs.closeSync(fd);
  }
}

// Updates the file IN PLACE, writing only the 4-byte fields named in `updates`. The helper (mode,
// via `pipewire_helper --mode`) may write other fields concurrently, so a read-modify-write of all
// 16 bytes could put a stale `mode` back (for instance un-muting a Mute). It also never truncates:
// fs.writeFileSync truncates to zero first, and a helper that has the file mapped (mmap,
// MAP_SHARED) faults with SIGBUS when it reads past the end of a shortened file. A file that does
// not exist, or is shorter than 16 bytes, is extended with zeros to the size the helper expects.
function updateStateFile(statePath, updates = {}) {
  const { O_RDWR, O_CREAT } = fs.constants;
  const fd = openStateFile(statePath, O_RDWR | O_CREAT);
  try {
    if (fs.fstatSync(fd).size < STATE_SIZE) {
      fs.ftruncateSync(fd, STATE_SIZE);
    }
    for (const [offset, value] of fieldWrites(updates)) {
      const field = Buffer.alloc(4);
      field.writeUInt32LE(value, 0);
      fs.writeSync(fd, field, 0, 4, offset);
    }
    const out = Buffer.alloc(STATE_SIZE, 0);
    fs.readSync(fd, out, 0, STATE_SIZE, 0);
    return out;
  } finally {
    fs.closeSync(fd);
  }
}

module.exports = {
  STATE_SIZE,
  PRESET_NAMES,
  isPreset,
  encodeState,
  readPreset,
  updateStateFile,
};
