'use strict';

// Fields owned by the service (GetStatus). They are never persisted locally.
const SERVICE_VOICE_PROFILE_KEYS = [
  'is_voice_profile_active',
  'stored_voice_profile_id',
  'voice_profile_error',
  'voice_profile_selected',
  'active_voice_profile_id',
  'has_voice_profile',
  // GetStatus: the active backend can apply a voice profile (absent = unknown, older service).
  'voice_profile_supported',
  // GetStatus: the APPLIED profile carries a microphone EQ (never read from the local file).
  'neural_eq_calibrated',
  // GetStatus, development only: 'pdfnet3-dev' | 'base' (anything else is dropped).
  'dev_base_model',
  // GetStatus: fixed DEV_MODEL_* code or null (free text, e.g. a path, is dropped).
  'dev_base_model_error',
];

const DEV_BASE_MODELS = new Set(['pdfnet3-dev', 'base']);
const DEV_BASE_MODEL_ERROR_RE = /^DEV_MODEL_[A-Z_]{1,48}$/;

// Service fields accepted only as real booleans; anything else is dropped (= unknown).
const BOOLEAN_SERVICE_KEYS = new Set([
  'is_voice_profile_active', 'has_voice_profile', 'voice_profile_supported', 'neural_eq_calibrated',
]);

function isObject(v) {
  return v !== null && typeof v === 'object' && !Array.isArray(v);
}

function pickServiceVoiceProfileFields(serviceStatus) {
  if (!isObject(serviceStatus)) return {};
  const out = {};
  for (const key of SERVICE_VOICE_PROFILE_KEYS) {
    if (!(key in serviceStatus) || serviceStatus[key] === undefined) continue;
    if (BOOLEAN_SERVICE_KEYS.has(key) && typeof serviceStatus[key] !== 'boolean') continue;
    if (key === 'dev_base_model' && !DEV_BASE_MODELS.has(serviceStatus[key])) continue;
    if (key === 'dev_base_model_error' && serviceStatus[key] !== null
      && !(typeof serviceStatus[key] === 'string' && DEV_BASE_MODEL_ERROR_RE.test(serviceStatus[key]))) continue;
    out[key] = serviceStatus[key];
  }
  return out;
}

// "A profile exists" is the service's answer (GetStatus), never the local file: the enrollment
// pipeline builds the profile in the service and does not write is_enrolled locally, and a local
// is_enrolled:true left by the old flow must not claim a profile the service does not hold.
function serviceHasVoiceProfile(serviceStatus) {
  if (!isObject(serviceStatus)) return false;
  const id = serviceStatus.stored_voice_profile_id;
  return (typeof id === 'string' && id.length > 0) || serviceStatus.has_voice_profile === true;
}

function mergeLocalAndServiceProfile(localProfile, serviceStatus) {
  return {
    // Service-owned fields left in the local file (e.g. an old neural_eq_calibrated) never count.
    ...stripServiceVoiceProfileFields(localProfile),
    ...pickServiceVoiceProfileFields(serviceStatus),
    is_enrolled: serviceHasVoiceProfile(serviceStatus),
  };
}

function stripServiceVoiceProfileFields(profile) {
  if (!isObject(profile)) return {};
  const out = { ...profile };
  for (const key of SERVICE_VOICE_PROFILE_KEYS) delete out[key];
  return out;
}

// Result of set_voice_profile: local profile + fresh service status + fixed-code error.
function buildSetVoiceProfileResult({ updated, serviceStatus, forwardError }) {
  const merged = mergeLocalAndServiceProfile(updated, serviceStatus);
  return forwardError ? { ...merged, voice_profile_error: forwardError } : merged;
}

// Maps a forward failure to a fixed code; never echoes error text (may contain the profile JSON).
function classifyForwardError(err) {
  const msg = err && typeof err.message === 'string' ? err.message : '';
  if (err && typeof err.code === 'string' && /^[A-Z0-9_]{1,64}$/.test(err.code)) return err.code;
  if (/^Daemon (unreachable|IPC timeout)/.test(msg)) return 'service_unavailable';
  if (/^Failed to parse daemon response/.test(msg)) return 'service_error';
  return 'service_rejected';
}

module.exports = {
  buildSetVoiceProfileResult,
  classifyForwardError,
  SERVICE_VOICE_PROFILE_KEYS,
  pickServiceVoiceProfileFields,
  mergeLocalAndServiceProfile,
  serviceHasVoiceProfile,
  stripServiceVoiceProfileFields,
};
