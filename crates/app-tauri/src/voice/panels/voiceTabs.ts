import type { EnrollErrorCode } from '../enrollmentTypes';
import { isBudgetError } from '../enrollmentErrors';
import type { JobOrigin } from '../hooks/useJobFeedback';

export const VOICE_TAB_IDS = ['enroll', 'gallery', 'calls', 'profile'] as const satisfies ReadonlyArray<JobOrigin>;
export type VoiceTabId = (typeof VOICE_TAB_IDS)[number];

/** UI preference only (which tab was open); no sample, take or profile data is stored locally. */
export const VOICE_TAB_STORAGE_KEY = 'clearcore_voice_active_tab';

/** A stored value is trusted only if it is one of the tab ids; anything else opens the first tab. */
export function parseVoiceTab(raw: unknown): VoiceTabId {
  return typeof raw === 'string' && (VOICE_TAB_IDS as ReadonlyArray<string>).includes(raw) ? (raw as VoiceTabId) : VOICE_TAB_IDS[0];
}

/** localStorage, or undefined where the accessor itself throws (blocked storage). */
export function browserStorage(): Storage | undefined {
  try {
    return typeof localStorage === 'undefined' ? undefined : localStorage;
  } catch {
    return undefined;
  }
}

export function loadVoiceTab(storage: Pick<Storage, 'getItem'> | undefined): VoiceTabId {
  try {
    return parseVoiceTab(storage?.getItem(VOICE_TAB_STORAGE_KEY));
  } catch {
    return VOICE_TAB_IDS[0];
  }
}

export function saveVoiceTab(id: VoiceTabId, storage: Pick<Storage, 'setItem'> | undefined): void {
  try {
    storage?.setItem(VOICE_TAB_STORAGE_KEY, id);
  } catch {
    // UI preference only
  }
}

/** The budget error asks the user to delete audio, so the card opens the gallery (spec §4.4). */
export function tabAfterError(current: VoiceTabId, code: EnrollErrorCode | null): VoiceTabId {
  return isBudgetError(code) ? 'gallery' : current;
}

/** Gallery: number of samples in the service. Calls: pending takes, only when there are any. */
export function voiceTabBadges(samplesCount: number, pendingTakes: number): { gallery: number; calls: number | undefined } {
  return { gallery: samplesCount, calls: pendingTakes > 0 ? pendingTakes : undefined };
}

/** Job feedback is shown in the tab whose action started the job. */
export function showsJobFeedback(tab: VoiceTabId, origin: JobOrigin | null): boolean {
  return origin === tab;
}
