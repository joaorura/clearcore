/*
 * Earlier builds kept voice samples, takes and an "enrolled" flag in localStorage. Samples now live
 * only in the service, so those keys are removed once; if any local sample existed, the user is told
 * ONCE that it has to be recorded again. The "notice shown" flag is a UI preference.
 */

export const LEGACY_VOICE_KEYS = [
  'clearcore_voice_profile_samples',
  'clearcore_voice_profile_enrolled',
  'clearcore_voice_intake_takes',
  'clearcore_voice_sample_count',
] as const;

export const LEGACY_NOTICE_SHOWN_KEY = 'clearcore_voice_legacy_notice_shown';

type KeyStorage = Pick<Storage, 'getItem' | 'setItem' | 'removeItem'>;

function hadSamples(samplesJson: string | null, count: string | null): boolean {
  if (count !== null && Number(count) > 0) return true;
  if (samplesJson === null) return false;
  try {
    const parsed: unknown = JSON.parse(samplesJson);
    return Array.isArray(parsed) ? parsed.length > 0 : true;
  } catch {
    return samplesJson.trim().length > 0; // corrupt but present: there was something
  }
}

/** Removes the legacy keys; true when local samples existed. Never throws. */
export function purgeLegacyVoiceKeys(storage: KeyStorage | undefined): boolean {
  if (!storage) return false;
  let existed = false;
  try {
    existed = hadSamples(storage.getItem('clearcore_voice_profile_samples'), storage.getItem('clearcore_voice_sample_count'));
  } catch {
    return false;
  }
  for (const key of LEGACY_VOICE_KEYS) {
    try {
      storage.removeItem(key);
    } catch {
      // blocked storage: nothing else to do
    }
  }
  return existed;
}

/** True exactly once when local samples were found (records that the notice was shown). */
export function takeLegacyNotice(hadLocalSamples: boolean, storage: KeyStorage | undefined): boolean {
  if (!hadLocalSamples || !storage) return false;
  try {
    if (storage.getItem(LEGACY_NOTICE_SHOWN_KEY) === 'true') return false;
    storage.setItem(LEGACY_NOTICE_SHOWN_KEY, 'true');
    return true;
  } catch {
    return false;
  }
}
