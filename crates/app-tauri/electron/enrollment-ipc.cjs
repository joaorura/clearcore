'use strict';

// Pure Electron-main module for the voice enrollment IPC. `sendIpcRequest` is injected.
// Raw PCM is never written to disk or logged; handlers never log names or error texts.

const os = require('node:os');

const MAX_SPEECH_SECONDS = 90;
const RATE = 48000;
const MAX_NAME = 64;
const MAX_LABEL = 128;
const MAX_META_BYTES = 256; // service limit on metadata, in UTF-8 bytes
// The service prefixes the job table: sample-job-N (sample/take ingestion), profile-job-N (build).
const JOB_ID_RE = /^(?:sample|profile)-job-[0-9]{1,9}$/;
const KNOWN_CODES = new Set([
  'ENROLL_CLIPPING', 'ENROLL_TOO_QUIET', 'ENROLL_TOO_LITTLE_SPEECH', 'ENROLL_MODEL_NOT_CONFIGURED',
  'ENROLL_BUDGET_EXCEEDED', 'ENROLL_INVALID_AUDIO', 'ENROLL_PAYLOAD_TOO_LARGE', 'ENROLL_JOB_NOT_FOUND',
  'ENROLL_BUSY', 'ENROLL_FAILED', 'ENROLL_BACKEND_UNSUPPORTED',
]);
const CONNECTION_CODES = new Set(['ECONNREFUSED', 'ENOENT', 'ECONNRESET', 'EPIPE', 'ETIMEDOUT']);

// Trim, drop orphan surrogates (serde_json rejects them), then cut by code points and by UTF-8 bytes
// without ever splitting a code point.
function sanitizeMeta(value, maxChars) {
  const points = Array.from(String(value == null ? '' : value).trim())
    .filter((ch) => !/^[\ud800-\udfff]$/.test(ch))
    .slice(0, maxChars);
  let bytes = 0;
  const out = [];
  for (const ch of points) {
    const b = Buffer.byteLength(ch, 'utf8');
    if (bytes + b > MAX_META_BYTES) break;
    bytes += b;
    out.push(ch);
  }
  return out.join('');
}

// The invoke payload is a structured-clone copy owned by this handler, so it is safe to wipe.
function wipePcm(pcm) {
  try {
    if (pcm instanceof ArrayBuffer) new Uint8Array(pcm).fill(0);
    else if (ArrayBuffer.isView(pcm)) new Uint8Array(pcm.buffer, pcm.byteOffset, pcm.byteLength).fill(0);
  } catch (_) { /* best effort */ }
}
// 'timeout': a job the service watchdog failed after 10 min (state 'failed', ENROLL_FAILED).
const STAGES = ['queued', 'denoise', 'trim', 'eq', 'enroll', 'apply', 'timeout'];
const STATES = ['running', 'done', 'failed'];

function invalidAudio(message) {
  const err = new Error(message);
  err.code = 'ENROLL_INVALID_AUDIO';
  return err;
}

function pcmToBuffer(pcm) {
  if (os.endianness() !== 'LE') throw invalidAudio('big-endian hosts are not supported');
  let buf;
  if (pcm instanceof Float32Array) {
    buf = Buffer.from(pcm.buffer, pcm.byteOffset, pcm.byteLength);
  } else if (pcm instanceof ArrayBuffer) {
    buf = Buffer.from(pcm, 0, pcm.byteLength);
  } else if (ArrayBuffer.isView(pcm) && pcm instanceof Uint8Array) {
    buf = Buffer.from(pcm.buffer, pcm.byteOffset, pcm.byteLength);
  } else {
    throw invalidAudio('pcm must be a Float32Array, ArrayBuffer or Uint8Array');
  }
  if (buf.length === 0 || buf.length % 4 !== 0) throw invalidAudio('pcm is empty or misaligned');
  if (buf.length / 4 > RATE * MAX_SPEECH_SECONDS) throw invalidAudio('pcm is longer than 90 s');
  // Copy to an aligned buffer to validate finiteness without depending on offset alignment.
  const view = new DataView(buf.buffer, buf.byteOffset, buf.byteLength);
  for (let i = 0; i < buf.length; i += 4) {
    if (!Number.isFinite(view.getFloat32(i, true))) throw invalidAudio('pcm has a non-finite sample');
  }
  return buf;
}

function buildAddVoiceSampleCommand({ pcm, sampleRate, name, device } = {}) {
  if (sampleRate !== RATE) throw invalidAudio('sampleRate must be 48000');
  const buf = pcmToBuffer(pcm);
  if (!device || typeof device.label !== 'string') throw invalidAudio('device label must be a string');
  if (typeof device.idHash !== 'string' || !/^[0-9a-f]{64}$/.test(device.idHash)) {
    throw invalidAudio('device id hash must be 64 lowercase hex chars');
  }
  return {
    AddVoiceSample: {
      name: sanitizeMeta(name, MAX_NAME),
      pcm_f32_le_b64: buf.toString('base64'),
      sample_rate: RATE,
      device_label: sanitizeMeta(device.label, MAX_LABEL),
      device_id_hash: device.idHash,
    },
  };
}

function buildBuildProfileCommand({ name } = {}) {
  return { BuildVoiceProfile: { name: sanitizeMeta(name, MAX_NAME) } };
}

function buildGetJobCommand({ jobId } = {}) {
  if (typeof jobId !== 'string' || !JOB_ID_RE.test(jobId)) throw new Error('invalid job id');
  return { GetEnrollmentJob: { job_id: jobId } };
}

function num(v, fallback = 0) {
  return typeof v === 'number' && Number.isFinite(v) ? v : fallback;
}

function mapJob(s) {
  const src = s && typeof s === 'object' ? s : {};
  if (typeof src.job_id !== 'string' || !JOB_ID_RE.test(src.job_id) || !STATES.includes(src.state)) {
    // Fail closed: a malformed job must stop the poller, not look like a running one.
    return {
      jobId: '', state: 'failed', stage: 'queued', errorCode: 'ENROLL_FAILED',
      remainingSeconds: null, sampleId: null, profileId: null, quality: null,
    };
  }
  const q = src.quality && typeof src.quality === 'object' ? src.quality : null;
  return {
    jobId: src.job_id,
    state: src.state,
    stage: STAGES.includes(src.stage) ? src.stage : 'queued',
    errorCode: typeof src.error_code === 'string' ? (KNOWN_CODES.has(src.error_code) ? src.error_code : 'ENROLL_FAILED') : null,
    remainingSeconds: Number.isFinite(src.remaining_seconds) ? src.remaining_seconds : null,
    sampleId: typeof src.sample_id === 'string' ? src.sample_id : null,
    profileId: typeof src.profile_id === 'string' ? src.profile_id : null,
    quality: q
      ? {
          peak: num(q.peak),
          rmsDbfs: num(q.rms_dbfs),
          activeFraction: num(q.active_fraction),
          speechSeconds: num(q.speech_seconds),
        }
      : null,
  };
}

const HASH_RE = /^[0-9a-f]{64}$/;
const hashOrNull = (v) => (typeof v === 'string' && HASH_RE.test(v) ? v : null);

function mapSampleList(s) {
  const src = s && typeof s === 'object' ? s : {};
  // A daemon from before the speech budget does not send `budget`: that is an outdated service,
  // not "0 s remaining". The budget below is then neutral and the UI shows the notice instead.
  const hasBudget = src.budget !== null && typeof src.budget === 'object' && !Array.isArray(src.budget);
  const b = hasBudget ? src.budget : {};
  return {
    serviceOutdated: !hasBudget,
    samples: (Array.isArray(src.samples) ? src.samples : [])
      .filter((x) => x && typeof x === 'object' && typeof x.id === 'string')
      .map((x) => ({
      id: x.id,
      name: typeof x.name === 'string' ? x.name : '',
      timestamp: String(x.timestamp == null ? '' : x.timestamp),
      speechSeconds: num(x.speech_seconds),
      deviceLabel: typeof x.device_label === 'string' ? x.device_label : '',
      usedInProfile: x.used_in_profile === true,
      needsReenroll: x.needs_reenroll === true,
      otherMicrophone: x.other_microphone === true,
      deviceIdHash: hashOrNull(x.device_id_hash),
    })),
    // Current device group (null/'' when an older service does not send it).
    selectedDeviceIdHash: hashOrNull(src.selected_device_id_hash),
    selectedDeviceLabel: typeof src.selected_device_label === 'string' ? sanitizeMeta(src.selected_device_label, MAX_LABEL) : '',
    budget: {
      usedSeconds: num(b.used_seconds),
      maxSeconds: num(b.max_seconds, MAX_SPEECH_SECONDS),
      remainingSeconds: num(b.remaining_seconds, hasBudget ? 0 : MAX_SPEECH_SECONDS),
    },
  };
}

function classifyEnrollError(err) {
  const code = err && typeof err.code === 'string' ? err.code : '';
  if (KNOWN_CODES.has(code)) return code;
  if (CONNECTION_CODES.has(code)) return 'SERVICE_UNAVAILABLE';
  const msg = err && typeof err.message === 'string' ? err.message : '';
  if (msg.startsWith('Daemon unreachable') || msg.startsWith('Daemon IPC timeout')) return 'SERVICE_UNAVAILABLE';
  return 'ENROLL_FAILED';
}

// The current service's own fixed INVALID_COMMAND refusals for these requests.
const CURRENT_INVALID_COMMAND_MESSAGES = new Set(['invalid sample metadata', 'invalid profile name']);

/**
 * AddVoiceSample / BuildVoiceProfile refused because the daemon does not know the request shape
 * (it predates the enrollment pipeline): JSON_PARSE_ERROR, an INVALID_COMMAND that is not one of
 * the current service's fixed refusals, or a serde "unknown field/variant". The user must restart
 * ClearCore, which ENROLL_FAILED would not say.
 */
function classifyStartError(err) {
  const code = err && typeof err.code === 'string' ? err.code : '';
  const msg = err && typeof err.message === 'string' ? err.message : '';
  if (code === 'JSON_PARSE_ERROR') return 'SERVICE_OUTDATED';
  if (code === 'INVALID_COMMAND' && !CURRENT_INVALID_COMMAND_MESSAGES.has(msg)) return 'SERVICE_OUTDATED';
  if ((code === '' || code === 'INVALID_COMMAND') && /unknown (field|variant)/.test(msg)) return 'SERVICE_OUTDATED';
  return classifyEnrollError(err);
}

function registerEnrollmentHandlers(ipcMain, { sendIpcRequest }) {
  const wrap = (channel, fn, classify = classifyEnrollError) => {
    ipcMain.handle(channel, async (_event, args) => {
      try {
        return await fn(args || {});
      } catch (err) {
        return { errorCode: classify(err) };
      }
    });
  };

  const jobIdOf = (res) => {
    if (!res || typeof res.job_id !== 'string' || !JOB_ID_RE.test(res.job_id)) {
      const err = new Error('malformed job id');
      err.code = 'ENROLL_FAILED';
      throw err;
    }
    return { jobId: res.job_id };
  };
  wrap('enrollment_add_sample', async (a) => {
    try {
      const cmd = buildAddVoiceSampleCommand(a);
      return jobIdOf(await sendIpcRequest(cmd, {}, 60000));
    } finally {
      wipePcm(a.pcm);
    }
  }, classifyStartError);
  wrap('enrollment_build_profile', async (a) => jobIdOf(await sendIpcRequest(buildBuildProfileCommand(a), {}, 5000)), classifyStartError);
  wrap('enrollment_get_job', async (a) => mapJob(await sendIpcRequest(buildGetJobCommand(a), {}, 5000)));
  wrap('enrollment_list_samples', async () => mapSampleList(await sendIpcRequest('ListVoiceSamples', {}, 5000)));
  wrap('enrollment_delete_sample', async (a) => {
    if (typeof a.id !== 'string' || a.id.length === 0) throw new Error('invalid sample id');
    await sendIpcRequest({ DeleteVoiceSample: { id: a.id } }, {}, 5000);
    return { success: true };
  });
}

module.exports = {
  MAX_SPEECH_SECONDS,
  RATE,
  buildAddVoiceSampleCommand,
  buildBuildProfileCommand,
  buildGetJobCommand,
  mapJob,
  mapSampleList,
  classifyEnrollError,
  classifyStartError,
  registerEnrollmentHandlers,
};
