import type { VoiceProfileStatus } from '../../types';
import type { EnrollErrorCode, EnrollmentJob, EnrollmentLabels, Quality } from '../enrollmentTypes';
import { enrollmentErrorCode, isBudgetError } from '../enrollmentErrors';
import { formatSecondsLocale } from '../format';

/*
 * Pure logic of the voice profile card. VoiceProfileCard.tsx re-exports every symbol here so
 * existing imports keep working.
 */

export function normalizeVoiceProfileStatus(res: unknown): VoiceProfileStatus {
  const r = (res ?? {}) as Partial<VoiceProfileStatus> & { profile?: Partial<VoiceProfileStatus> };
  const src = r.profile ?? r;
  return {
    is_enrolled: Boolean(src.is_enrolled),
    active_samples_count: src.active_samples_count ?? 0,
    embedding_dim: src.embedding_dim ?? 0,
    neural_eq_calibrated: Boolean(src.neural_eq_calibrated),
    gain_boost_db: src.gain_boost_db,
    is_voice_profile_active: src.is_voice_profile_active,
    stored_voice_profile_id: src.stored_voice_profile_id,
    voice_profile_error: src.voice_profile_error,
    voice_profile_selected: src.voice_profile_selected,
    active_voice_profile_id: src.active_voice_profile_id,
  };
}

export type VoiceProfileActivationState = 'active' | 'stored_not_applied' | 'none';

export type VoiceProfileStatusLabelKey = 'active' | 'storedNotApplied' | 'enrolledUnconfirmed' | 'none';

/** Only 'active' may be shown as active; enrolled without confirmation is neutral. */
export function voiceProfileStatusLabelKey(status: VoiceProfileStatus): VoiceProfileStatusLabelKey {
  const state = voiceProfileActivationState(status);
  if (state === 'active') return 'active';
  if (state === 'stored_not_applied') return 'storedNotApplied';
  return status.is_enrolled ? 'enrolledUnconfirmed' : 'none';
}

/** Stored must never be presented as applied: 'active' needs explicit confirmation. */
export function voiceProfileActivationState(status: VoiceProfileStatus): VoiceProfileActivationState {
  if (status.is_voice_profile_active === true) return 'active';
  const hasStoredId =
    typeof status.stored_voice_profile_id === 'string' && status.stored_voice_profile_id.length > 0;
  if (hasStoredId || (status.is_enrolled && status.is_voice_profile_active === false)) {
    return 'stored_not_applied';
  }
  return 'none';
}

export function mergeVoiceProfileStatus(
  initial: VoiceProfileStatus,
  res: unknown,
  loadedSamplesCount: number,
): VoiceProfileStatus {
  const r = (res ?? {}) as Partial<VoiceProfileStatus> & { profile?: Partial<VoiceProfileStatus> };
  const raw = r.profile ?? r;
  if (!raw || typeof raw !== 'object' || typeof raw.is_enrolled !== 'boolean') return initial;
  const prof = normalizeVoiceProfileStatus(res);
  return {
    ...initial,
    is_enrolled: prof.is_enrolled,
    neural_eq_calibrated: prof.neural_eq_calibrated,
    embedding_dim: raw.embedding_dim ?? initial.embedding_dim,
    gain_boost_db: raw.gain_boost_db ?? initial.gain_boost_db,
    is_voice_profile_active: prof.is_voice_profile_active,
    stored_voice_profile_id: prof.stored_voice_profile_id,
    voice_profile_error: prof.voice_profile_error,
    voice_profile_selected: prof.voice_profile_selected,
    active_voice_profile_id: prof.active_voice_profile_id,
    active_samples_count: loadedSamplesCount > 0 ? loadedSamplesCount : prof.active_samples_count,
  };
}

const SERVICE_VOICE_PROFILE_KEYS = [
  'is_voice_profile_active',
  'stored_voice_profile_id',
  'voice_profile_error',
  'voice_profile_selected',
  'active_voice_profile_id',
] as const;

/** Service-owned fields: the renderer never resends nor inherits them. */
export function stripServiceVoiceProfileKeys(status: VoiceProfileStatus): VoiceProfileStatus {
  const out: VoiceProfileStatus = { ...status };
  for (const key of SERVICE_VOICE_PROFILE_KEYS) delete out[key];
  return out;
}

/**
 * Applies the result of set_voice_profile: local fields come from what the user did, service
 * fields come ONLY from the result (missing/offline result => cleared, never kept as active).
 */
export function applySetVoiceProfileResult(
  prev: VoiceProfileStatus,
  localStatus: VoiceProfileStatus,
  res: unknown,
): VoiceProfileStatus {
  const base = { ...stripServiceVoiceProfileKeys(prev), ...stripServiceVoiceProfileKeys(localStatus) };
  const r = (res ?? {}) as { profile?: unknown };
  const raw = (r.profile ?? res) as Record<string, unknown> | null | undefined;
  if (!raw || typeof raw !== 'object') return base;
  const svc = normalizeVoiceProfileStatus(raw);
  return {
    ...base,
    is_voice_profile_active: typeof svc.is_voice_profile_active === 'boolean' ? svc.is_voice_profile_active : undefined,
    stored_voice_profile_id: svc.stored_voice_profile_id,
    voice_profile_error: svc.voice_profile_error,
    voice_profile_selected: svc.voice_profile_selected,
    active_voice_profile_id: svc.active_voice_profile_id,
  };
}

export type VoiceProfileErrorKey =
  | 'errorServiceUnavailable' | 'errorServiceError' | 'errorServiceRejected' | 'errorNotApplicable'
  | 'errorClearFailed' | 'errorNoProfileStore' | 'errorInvalidProfile' | 'errorPersistFailed'
  | 'errorNotApplied' | 'errorLoadFailed' | 'errorRestoreFailed' | 'errorUnknown';

const VOICE_PROFILE_ERROR_KEYS: Record<string, VoiceProfileErrorKey> = {
  service_unavailable: 'errorServiceUnavailable',
  service_error: 'errorServiceError',
  service_rejected: 'errorServiceRejected',
  VOICE_PROFILE_NOT_APPLICABLE: 'errorNotApplicable',
  VOICE_PROFILE_CLEAR_FAILED: 'errorClearFailed',
  NO_PROFILE_STORE: 'errorNoProfileStore',
  'Invalid voice profile': 'errorInvalidProfile',
  'Failed to persist voice profile': 'errorPersistFailed',
  'The backend could not apply the stored voice profile': 'errorNotApplied',
  'The stored voice profile failed validation or has insecure permissions': 'errorLoadFailed',
  'The previous voice profile could not be restored on the backend': 'errorRestoreFailed',
  'The active backend could not return to the neutral voice': 'errorClearFailed',
  'The active backend cannot apply this voice profile': 'errorNotApplicable',
  'Voice profile storage is not configured': 'errorNoProfileStore',
};

/** Never returns service free text: unknown input maps to errorUnknown. */
export function voiceProfileErrorKey(code: unknown): VoiceProfileErrorKey {
  if (typeof code !== 'string') return 'errorUnknown';
  return Object.prototype.hasOwnProperty.call(VOICE_PROFILE_ERROR_KEYS, code)
    ? VOICE_PROFILE_ERROR_KEYS[code]
    : 'errorUnknown';
}

const ENROLLMENT_LABEL_KEYS = [
  'budgetTitle', 'budgetUsed', 'budgetRemaining', 'seconds', 'budgetExceededTitle', 'budgetExceededBody',
  'deleteAction', 'deleting', 'otherMicrophone', 'needsReenroll', 'usedInProfile', 'notUsed',
  'stageQueued', 'stageDenoise', 'stageTrim', 'stageEq', 'stageEnroll', 'stageApply',
  'jobDone', 'jobFailed', 'devModelNotice', 'qualityPeak', 'qualityLevel', 'qualitySpeech',
] as const satisfies ReadonlyArray<Exclude<keyof EnrollmentLabels, 'errors'>>;

const ENROLLMENT_ERROR_KEYS = [
  'ENROLL_CLIPPING', 'ENROLL_TOO_QUIET', 'ENROLL_TOO_LITTLE_SPEECH', 'ENROLL_MODEL_NOT_CONFIGURED',
  'ENROLL_BUDGET_EXCEEDED', 'ENROLL_INVALID_AUDIO', 'ENROLL_PAYLOAD_TOO_LARGE',
  'ENROLL_JOB_NOT_FOUND', 'ENROLL_BUSY', 'ENROLL_FAILED', 'SERVICE_UNAVAILABLE', 'UNKNOWN',
] as const satisfies ReadonlyArray<keyof EnrollmentLabels['errors']>;

/** Fills every EnrollmentLabels field from i18n (`voiceProfile.enrollment.*`). */
export function buildEnrollmentLabels(t: (path: string) => string): EnrollmentLabels {
  const labels = {} as Omit<EnrollmentLabels, 'errors'>;
  for (const key of ENROLLMENT_LABEL_KEYS) labels[key] = t(`voiceProfile.enrollment.${key}`);
  const errors = {} as EnrollmentLabels['errors'];
  for (const code of ENROLLMENT_ERROR_KEYS) errors[code] = t(`voiceProfile.enrollment.errors.${code}`);
  return { ...labels, errors };
}

export type JobOutcome =
  | { kind: 'done'; quality: Quality | null }
  | { kind: 'show-budget-error'; remainingSeconds: number | null }
  | { kind: 'show-error'; code: EnrollErrorCode };

/** What the card does once a sample/profile job has finished. */
export function nextStepAfterJob(job: EnrollmentJob): JobOutcome {
  if (job.state === 'done') return { kind: 'done', quality: job.quality };
  if (isBudgetError(job.errorCode)) return { kind: 'show-budget-error', remainingSeconds: job.remainingSeconds };
  return { kind: 'show-error', code: job.errorCode ?? 'ENROLL_FAILED' };
}

/** Only the budget error asks the user to free speech by deleting samples (spec §4.4). */
export function shouldOpenGalleryOnError(code: EnrollErrorCode | null): boolean {
  return isBudgetError(code);
}

/**
 * A dynamic take that the service did not record for lack of margin shows no error (spec §4.4);
 * an explicit user action (approve) that fails shows its error.
 */
export function shouldShowTakeError(res: unknown): boolean {
  if (typeof res !== 'object' || res === null) return false;
  const r = res as { recorded?: unknown; reason?: unknown };
  if (r.recorded === false && r.reason === 'budget') return false;
  return enrollmentErrorCode(res) !== null;
}

/** Seconds with the decimal separator of the locale (comma in pt-BR). */
export function formatSecondsForLocale(seconds: number, locale: string): string {
  return formatSecondsLocale(seconds, locale);
}

/** Interpolates `{remaining}` with the localized figure; leaves the template alone when unknown. */
export function interpolateBudgetBody(template: string, remainingSeconds: number | null, locale: string): string {
  if (remainingSeconds === null) return template;
  return template.split('{remaining}').join(formatSecondsForLocale(remainingSeconds, locale));
}

export type Translate = (path: string, params?: Record<string, string | number>) => string;

export type TakeApprovalOutcome =
  | { kind: 'approved' }
  | { kind: 'budget'; remainingSeconds: number | null }
  | { kind: 'error'; code: EnrollErrorCode | null };

/** What approving a call take means for the card (the service rechecks the speech budget). */
export function classifyTakeApproval(res: unknown): TakeApprovalOutcome {
  if (!shouldShowTakeError(res)) return { kind: 'approved' };
  const code = enrollmentErrorCode(res);
  if (isBudgetError(code)) {
    const remaining = (res as { remainingSeconds?: unknown }).remainingSeconds;
    return { kind: 'budget', remainingSeconds: typeof remaining === 'number' ? remaining : null };
  }
  return { kind: 'error', code };
}
