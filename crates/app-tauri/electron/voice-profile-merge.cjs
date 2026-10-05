'use strict';

// Fields owned by the service (GetStatus). They are never persisted locally.
const SERVICE_VOICE_PROFILE_KEYS = [
  'is_voice_profile_active',
  'stored_voice_profile_id',
  'voice_profile_error',
  'voice_profile_selected',
  'active_voice_profile_id',
];

function isObject(v) {
  return v !== null && typeof v === 'object' && !Array.isArray(v);
}

function pickServiceVoiceProfileFields(serviceStatus) {
  if (!isObject(serviceStatus)) return {};
  const out = {};
  for (const key of SERVICE_VOICE_PROFILE_KEYS) {
    if (!(key in serviceStatus) || serviceStatus[key] === undefined) continue;
    if (key === 'is_voice_profile_active' && typeof serviceStatus[key] !== 'boolean') continue;
    out[key] = serviceStatus[key];
  }
  return out;
}

function mergeLocalAndServiceProfile(localProfile, serviceStatus) {
  return { ...(isObject(localProfile) ? localProfile : {}), ...pickServiceVoiceProfileFields(serviceStatus) };
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
  stripServiceVoiceProfileFields,
};
