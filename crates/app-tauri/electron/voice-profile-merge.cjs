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

module.exports = {
  SERVICE_VOICE_PROFILE_KEYS,
  pickServiceVoiceProfileFields,
  mergeLocalAndServiceProfile,
  stripServiceVoiceProfileFields,
};
