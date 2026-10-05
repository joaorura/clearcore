'use strict';

// Pure Electron-main module for the voice enrollment IPC. `sendIpcRequest` is injected.
// Raw PCM is never written to disk or logged; handlers never log names or error texts.

const os = require('node:os');

const MAX_SPEECH_SECONDS = 90;
const RATE = 48000;
const MAX_NAME = 64;
const MAX_LABEL = 128;
const STAGES = ['queued', 'denoise', 'trim', 'eq', 'enroll', 'apply'];
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
      name: String(name == null ? '' : name).trim().slice(0, MAX_NAME),
      pcm_f32_le_b64: buf.toString('base64'),
      sample_rate: RATE,
      device_label: device.label.trim().slice(0, MAX_LABEL),
      device_id_hash: device.idHash,
    },
  };
}

function buildBuildProfileCommand({ name } = {}) {
  return { BuildVoiceProfile: { name: String(name == null ? '' : name).trim().slice(0, MAX_NAME) } };
}

function buildGetJobCommand({ jobId } = {}) {
  if (typeof jobId !== 'string' || !/^job-[0-9]+$/.test(jobId)) throw new Error('invalid job id');
  return { GetEnrollmentJob: { job_id: jobId } };
}

function num(v, fallback = 0) {
  return typeof v === 'number' && Number.isFinite(v) ? v : fallback;
}

function mapJob(s) {
  const src = s && typeof s === 'object' ? s : {};
  const q = src.quality && typeof src.quality === 'object' ? src.quality : null;
  return {
    jobId: typeof src.job_id === 'string' ? src.job_id : '',
    state: STATES.includes(src.state) ? src.state : 'running',
    stage: STAGES.includes(src.stage) ? src.stage : 'queued',
    errorCode: typeof src.error_code === 'string' ? src.error_code : null,
    remainingSeconds: typeof src.remaining_seconds === 'number' ? src.remaining_seconds : null,
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

function mapSampleList(s) {
  const src = s && typeof s === 'object' ? s : {};
  const b = src.budget && typeof src.budget === 'object' ? src.budget : {};
  return {
    samples: (Array.isArray(src.samples) ? src.samples : []).map((x) => ({
      id: String(x.id),
      name: typeof x.name === 'string' ? x.name : '',
      timestamp: String(x.timestamp == null ? '' : x.timestamp),
      speechSeconds: num(x.speech_seconds),
      deviceLabel: typeof x.device_label === 'string' ? x.device_label : '',
      usedInProfile: x.used_in_profile === true,
      needsReenroll: x.needs_reenroll === true,
      otherMicrophone: x.other_microphone === true,
    })),
    budget: {
      usedSeconds: num(b.used_seconds),
      maxSeconds: num(b.max_seconds, MAX_SPEECH_SECONDS),
      remainingSeconds: num(b.remaining_seconds),
    },
  };
}

function classifyEnrollError(err) {
  if (err && typeof err.code === 'string' && /^ENROLL_[A-Z_]+$/.test(err.code)) return err.code;
  const msg = err && typeof err.message === 'string' ? err.message : '';
  if (/Daemon unreachable|Daemon IPC timeout|ECONNREFUSED|ENOENT|ECONNRESET|EPIPE/.test(msg)) {
    return 'SERVICE_UNAVAILABLE';
  }
  return 'ENROLL_FAILED';
}

function registerEnrollmentHandlers(ipcMain, { sendIpcRequest }) {
  const wrap = (channel, fn) => {
    ipcMain.handle(channel, async (_event, args) => {
      try {
        return await fn(args || {});
      } catch (err) {
        return { errorCode: classifyEnrollError(err) };
      }
    });
  };

  wrap('enrollment_add_sample', async (a) => {
    const cmd = buildAddVoiceSampleCommand(a);
    const res = await sendIpcRequest(cmd, {}, 60000);
    return { jobId: res && res.job_id };
  });
  wrap('enrollment_build_profile', async (a) => {
    const res = await sendIpcRequest(buildBuildProfileCommand(a), {}, 5000);
    return { jobId: res && res.job_id };
  });
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
  registerEnrollmentHandlers,
};
