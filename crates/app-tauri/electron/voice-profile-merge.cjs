'use strict';

// Fields owned by the service (GetStatus). They are never persisted locally.
const SERVICE_VOICE_PROFILE_KEYS = [
  'is_voice_profile_active',
  'stored_voice_profile_id',
  'voice_profile_error',
  'voice_profile_selected',
  'active_voice_profile_id',
  'has_voice_profile',
];

function isObject(v) {
  return v !== null && typeof v === 'object' && !Array.isArray(v);
}

function pickServiceVoiceProfileFields(serviceStatus) {
  if (!isObject(serviceStatus)) return {};
  const out = {};
  for (const key of SERVICE_VOICE_PROFILE_KEYS) {
    if (!(key in serviceStatus) || serviceStatus[key] === undefined) continue;
    if ((key === 'is_voice_profile_active' || key === 'has_voice_profile') && typeof serviceStatus[key] !== 'boolean') continue;
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
    ...(isObject(localProfile) ? localProfile : {}),
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
