import type { VoiceProfileStatus } from '../../types';
import type { EnrollErrorCode, EnrollmentJob, EnrollmentLabels, Quality, ServiceSample } from '../enrollmentTypes';
import { enrollmentErrorCode, errorLabel, isBudgetError } from '../enrollmentErrors';
import { formatDecimalLocale, formatSecondsLocale } from '../format';

/*
 * Pure logic of the voice profile card. VoiceProfileCard.tsx re-exports every symbol here so
 * existing imports keep working.
 */

/**
 * A profile exists when the SERVICE says so (GetStatus `stored_voice_profile_id` not empty or
 * `has_voice_profile`). A local `is_enrolled` is never trusted: the enrollment pipeline builds the
 * profile in the service, and a stale `is_enrolled: true` from the old local flow must not claim
 * a profile the service does not hold.
 */
export function hasServiceVoiceProfile(status: Partial<VoiceProfileStatus> | null | undefined): boolean {
  if (!status || typeof status !== 'object') return false;
  const storedId = status.stored_voice_profile_id;
  const activeId = status.active_voice_profile_id;
  return (typeof storedId === 'string' && storedId.length > 0)
    || (typeof activeId === 'string' && activeId.length > 0)
    || status.has_voice_profile === true
    || status.is_voice_profile_active === true;
}

export function normalizeVoiceProfileStatus(res: unknown): VoiceProfileStatus {
  const r = (res ?? {}) as Partial<VoiceProfileStatus> & { profile?: Partial<VoiceProfileStatus>; voice_samples_count?: number };
  const src = r.profile ?? r;
  const samplesCount = typeof (src as { voice_samples_count?: number }).voice_samples_count === 'number'
    ? (src as { voice_samples_count: number }).voice_samples_count
    : (src.active_samples_count ?? 0);
  return {
    is_enrolled: hasServiceVoiceProfile(src),
    active_samples_count: samplesCount,
    embedding_dim: src.embedding_dim ?? 0,
    neural_eq_calibrated: Boolean(src.neural_eq_calibrated),
    gain_boost_db: src.gain_boost_db,
    is_voice_profile_active: src.is_voice_profile_active,
    stored_voice_profile_id: src.stored_voice_profile_id,
    voice_profile_error: src.voice_profile_error,
    voice_profile_selected: src.voice_profile_selected,
    active_voice_profile_id: src.active_voice_profile_id,
    has_voice_profile: typeof src.has_voice_profile === 'boolean' ? src.has_voice_profile : undefined,
    voice_profile_supported: typeof src.voice_profile_supported === 'boolean' ? src.voice_profile_supported : undefined,
    dev_base_model: devBaseModel(src.dev_base_model),
    dev_base_model_error: devBaseModelError(src.dev_base_model_error),
    voice_isolation_enabled: typeof src.voice_isolation_enabled === 'boolean' ? src.voice_isolation_enabled : undefined,
  };
}

/** Only the two documented values are accepted; anything else is unknown. */
export function devBaseModel(value: unknown): VoiceProfileStatus['dev_base_model'] {
  return value === 'pdfnet3-dev' || value === 'base' ? value : undefined;
}

/** Only a fixed `DEV_MODEL_*` code is accepted (never free text such as a path). */
export function devBaseModelError(value: unknown): string | null | undefined {
  if (value === null) return null;
  return typeof value === 'string' && /^DEV_MODEL_[A-Z_]{1,48}$/.test(value) ? value : undefined;
}

/** The service said the active isolation model cannot apply a voice profile (absent = unknown). */
export function voiceProfileUnsupported(status: Partial<VoiceProfileStatus> | null | undefined): boolean {
  return status?.voice_profile_supported === false;
}

export type VoiceProfileActivationState = 'active' | 'stored_not_applied' | 'none';

export type VoiceProfileStatusLabelKey = 'active' | 'storedNotApplied' | 'none';

/** Only 'active' may be shown as active; a profile the service holds but did not apply is neutral. */
export function voiceProfileStatusLabelKey(status: VoiceProfileStatus): VoiceProfileStatusLabelKey {
  const state = voiceProfileActivationState(status);
  if (state === 'active') return 'active';
  if (state === 'stored_not_applied') return 'storedNotApplied';
  return 'none';
}

/** Stored must never be presented as applied: 'active' needs explicit confirmation. */
export function voiceProfileActivationState(status: VoiceProfileStatus): VoiceProfileActivationState {
  if (status.is_voice_profile_active === true) return 'active';
  return hasServiceVoiceProfile(status) ? 'stored_not_applied' : 'none';
}

const SERVICE_STATUS_KEYS_IN_RESPONSE = [
  'stored_voice_profile_id',
  'has_voice_profile',
  'is_voice_profile_active',
  'active_voice_profile_id',
  'voice_samples_count',
  'neural_eq_calibrated',
  'voice_isolation_enabled',
] as const;

export function mergeVoiceProfileStatus(
  initial: VoiceProfileStatus,
  res: unknown,
  loadedSamplesCount: number,
): VoiceProfileStatus {
  const r = (res ?? {}) as Partial<VoiceProfileStatus> & { profile?: Partial<VoiceProfileStatus> };
  const raw = r.profile ?? r;
  if (!raw || typeof raw !== 'object') return initial;
  const fromService = typeof raw.is_enrolled === 'boolean' || SERVICE_STATUS_KEYS_IN_RESPONSE.some((k) => k in raw);
  if (!fromService) return initial;
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
    has_voice_profile: prof.has_voice_profile,
    voice_profile_supported: prof.voice_profile_supported,
    dev_base_model: prof.dev_base_model,
    dev_base_model_error: prof.dev_base_model_error,
    voice_isolation_enabled: prof.voice_isolation_enabled,
    active_samples_count: loadedSamplesCount > 0 ? loadedSamplesCount : prof.active_samples_count,
  };
}

const SERVICE_VOICE_PROFILE_KEYS = [
  'is_voice_profile_active',
  'stored_voice_profile_id',
  'voice_profile_error',
  'voice_profile_selected',
  'active_voice_profile_id',
  'has_voice_profile',
  'voice_profile_supported',
  'neural_eq_calibrated',
  'voice_samples_count',
  'dev_base_model',
  'dev_base_model_error',
  'voice_isolation_enabled',
] as const;

/** Service-owned fields: the renderer never resends nor inherits them. */
export function stripServiceVoiceProfileKeys(status: VoiceProfileStatus): VoiceProfileStatus {
  const out: VoiceProfileStatus = { ...status };
  for (const key of SERVICE_VOICE_PROFILE_KEYS) delete (out as unknown as Record<string, unknown>)[key];
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
  // Without a service answer no profile is known to exist (the local flag is never trusted).
  const base = { ...stripServiceVoiceProfileKeys(prev), ...stripServiceVoiceProfileKeys(localStatus), is_enrolled: false };
  const r = (res ?? {}) as { profile?: unknown };
  const raw = (r.profile ?? res) as Record<string, unknown> | null | undefined;
  if (!raw || typeof raw !== 'object') return base;
  const svc = normalizeVoiceProfileStatus(raw);
  return {
    ...base,
    is_enrolled: svc.is_enrolled,
    has_voice_profile: svc.has_voice_profile,
    is_voice_profile_active: typeof svc.is_voice_profile_active === 'boolean' ? svc.is_voice_profile_active : undefined,
    stored_voice_profile_id: svc.stored_voice_profile_id,
    voice_profile_error: svc.voice_profile_error,
    voice_profile_selected: svc.voice_profile_selected,
    active_voice_profile_id: svc.active_voice_profile_id,
    voice_profile_supported: svc.voice_profile_supported,
    neural_eq_calibrated: svc.neural_eq_calibrated,
    dev_base_model: svc.dev_base_model,
    dev_base_model_error: svc.dev_base_model_error,
    voice_isolation_enabled: svc.voice_isolation_enabled,
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
  'stageQueued', 'stageDenoise', 'stageTrim', 'stageEq', 'stageEnroll', 'stageApply', 'stageTimeout',
  'jobDone', 'jobFailed', 'devModelNotice', 'devIsolationModelNotice', 'devIsolationModelError', 'qualityPeak', 'qualityLevel', 'qualitySpeech',
] as const satisfies ReadonlyArray<Exclude<keyof EnrollmentLabels, 'errors'>>;

const ENROLLMENT_ERROR_KEYS = [
  'ENROLL_CLIPPING', 'ENROLL_TOO_QUIET', 'ENROLL_TOO_LITTLE_SPEECH', 'ENROLL_MODEL_NOT_CONFIGURED',
  'ENROLL_BUDGET_EXCEEDED', 'ENROLL_INVALID_AUDIO', 'ENROLL_PAYLOAD_TOO_LARGE',
  'ENROLL_JOB_NOT_FOUND', 'ENROLL_BUSY', 'ENROLL_FAILED', 'SERVICE_UNAVAILABLE', 'SERVICE_OUTDATED', 'UNKNOWN',
  'ENROLL_BACKEND_UNSUPPORTED',
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
  | { kind: 'show-error'; code: EnrollErrorCode; quality?: Quality };

/** What the card does once a sample/profile job has finished. */
export function nextStepAfterJob(job: EnrollmentJob): JobOutcome {
  if (job.state === 'done') return { kind: 'done', quality: job.quality };
  if (isBudgetError(job.errorCode)) return { kind: 'show-budget-error', remainingSeconds: job.remainingSeconds };
  const code = job.errorCode ?? 'ENROLL_FAILED';
  return job.quality ? { kind: 'show-error', code, quality: job.quality } : { kind: 'show-error', code };
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
  /** The service did not record it for lack of margin (spec §4.4): no flash, no error. */
  | { kind: 'not-recorded' }
  | { kind: 'budget'; remainingSeconds: number | null }
  | { kind: 'error'; code: EnrollErrorCode | null };

/** What approving a call take means for the card (the service rechecks the speech budget). */
export function classifyTakeApproval(res: unknown): TakeApprovalOutcome {
  if (!shouldShowTakeError(res)) {
    const recorded = typeof res === 'object' && res !== null ? (res as { recorded?: unknown }).recorded : undefined;
    return recorded === false ? { kind: 'not-recorded' } : { kind: 'approved' };
  }
  const code = enrollmentErrorCode(res);
  if (isBudgetError(code)) {
    const remaining = (res as { remainingSeconds?: unknown }).remainingSeconds;
    return { kind: 'budget', remainingSeconds: typeof remaining === 'number' ? remaining : null };
  }
  return { kind: 'error', code };
}

/**
 * startCapture() returned without recording and without a physical-microphone error: tell the
 * user when another start was in flight ('busy') or the start was cancelled without them asking.
 */
export function shouldReportUnstartedCapture(
  status: 'started' | 'busy' | 'cancelled' | 'failed',
  cancelRequested: boolean,
): boolean {
  if (status === 'busy') return true;
  if (status === 'cancelled') return !cancelRequested;
  return false;
}

/**
 * Error label by the job's origin: ENROLL_TOO_LITTLE_SPEECH from a profile BUILD means the
 * current microphone's samples do not add up to enough speech (record more with it), which is not
 * the same as one short sample.
 */
export function errorLabelForJob(
  code: EnrollErrorCode | null,
  kind: 'sample' | 'build' | 'take',
  labels: EnrollmentLabels,
  t: Translate,
): string {
  if (kind === 'build' && code === 'ENROLL_TOO_LITTLE_SPEECH') return t('voiceProfile.buildTooLittleSpeech');
  return errorLabel(code, labels);
}

/** Samples the current profile is built from (not other-microphone nor re-record ones). */
export function samplesUsedInProfile(samples: ServiceSample[]): number {
  return samples.filter((s) => s.usedInProfile).length;
}

/** The measured value behind a quality error (spec §9), or null when the service sent none. */
export function measuredQualityText(code: EnrollErrorCode | null, quality: Quality | null | undefined, t: Translate, locale: string): string | null {
  if (!quality) return null;
  switch (code) {
    case 'ENROLL_CLIPPING':
      return t('voiceProfile.measuredPeak', { peak: formatDecimalLocale(quality.peak, 2, locale) });
    case 'ENROLL_TOO_QUIET':
      return t('voiceProfile.measuredLevel', { db: formatDecimalLocale(quality.rmsDbfs, 1, locale) });
    case 'ENROLL_TOO_LITTLE_SPEECH':
      return t('voiceProfile.measuredSpeech', { sec: formatSecondsLocale(quality.speechSeconds, locale) });
    default:
      return null;
  }
}

/** Samples of the current microphone group that have audio: what a build would use. */
export function profileSampleIds(samples: ServiceSample[]): string[] {
  return samples.filter((x) => !x.otherMicrophone && !x.needsReenroll).map((x) => x.id);
}

/**
 * The applied profile is out of date when the current group's samples changed (added or deleted)
 * since it was built in this session. Unknown build (null) is never reported. The card only says
 * so; rebuilding stays the explicit button (spec §4.4).
 */
export function profileIsStale(usedSampleIdsNow: string[], idsAtBuild: string[] | null): boolean {
  if (idsAtBuild === null) return false;
  const now = new Set(usedSampleIdsNow);
  const then = new Set(idsAtBuild);
  if (now.size !== then.size) return true;
  for (const id of now) if (!then.has(id)) return true;
  return false;
}

/**
 * The "profile out of date" notice: only for a profile the service holds, once the sample list is
 * known (null while it was not loaded) and changed since the build of this session.
 */
export function showStaleProfileNotice(
  status: VoiceProfileStatus,
  usedSampleIdsNow: string[] | null,
  idsAtBuild: string[] | null,
): boolean {
  return hasServiceVoiceProfile(status) && usedSampleIdsNow !== null && profileIsStale(usedSampleIdsNow, idsAtBuild);
}
