import React, { useState, useRef, useEffect, useCallback, useMemo } from 'react';
import { useI18n } from './i18n';
import { invokeBridge } from './bridge';
import type { CallSuggestionTake, VoiceProfileStatus, InputDeviceInfo } from './types';
import type { EnrollErrorCode, EnrollmentJob, EnrollmentLabels, Quality, SampleList } from './voice/enrollmentTypes';
import { enrollmentErrorCode, errorLabel, isBudgetError } from './voice/enrollmentErrors';
import { addSample, buildProfile, deleteSample, listSamples, waitForJob } from './voice/enrollmentClient';
import { DevModelNotice } from './voice/DevModelNotice';
import { VoiceBudgetMeter } from './voice/VoiceBudgetMeter';
import { VoiceSampleGallery } from './voice/VoiceSampleGallery';
import { BudgetErrorBanner } from './voice/BudgetErrorBanner';
import { EnrollmentJobStatus } from './voice/EnrollmentJobStatus';
import { formatSeconds } from './voice/speechBudget';
import { MAX_RECORD_SECONDS, MIN_RECORD_SECONDS } from './voice/enrollmentTypes';
import type { CapturedPcm, DeviceInfo } from './voice/enrollmentTypes';
import { acquireRawPhysicalStream, PhysicalMicUnavailableError } from './voice/captureDevice';
import { PcmRecorder } from './voice/pcmCapture';

export interface VoiceProfileCardProps {
  selectedInputId?: string;
  virtualMicPresent?: boolean;
  inputDevices?: InputDeviceInfo[];
}

// Capture helpers live in ./voice/captureDevice; re-exported so existing imports keep working.
export { isVirtualOrLoopbackAudioDevice, resolvePhysicalAudioDevice } from './voice/captureDevice';

const MIN_RECORDING_SECONDS = MIN_RECORD_SECONDS;
const MAX_RECORDING_SECONDS = MAX_RECORD_SECONDS;

/** UI preference only (open questions vs reading); no sample, take or profile data is stored locally. */
const STORAGE_READING_MODE_KEY = 'clearcore_voice_reading_mode';

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
  'ENROLL_JOB_NOT_FOUND', 'ENROLL_FAILED', 'SERVICE_UNAVAILABLE', 'UNKNOWN',
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

/** formatSeconds() always uses a point; pt-BR shows a decimal comma. */
export function formatSecondsForLocale(seconds: number, locale: string): string {
  const s = formatSeconds(seconds);
  return locale.toLowerCase().startsWith('pt') ? s.replace('.', ',') : s;
}

/** Interpolates `{remaining}` with the localized figure; leaves the template alone when unknown. */
export function interpolateBudgetBody(template: string, remainingSeconds: number | null, locale: string): string {
  if (remainingSeconds === null) return template;
  return template.split('{remaining}').join(formatSecondsForLocale(remainingSeconds, locale));
}

export const VoiceProfileCard: React.FC<VoiceProfileCardProps> = ({
  selectedInputId,
  virtualMicPresent: _virtualMicPresent,
  inputDevices,
}) => {
  const { t, locale } = useI18n();

  // Profile Status: neutral until the service reports it (no invented EQ, gain or embedding).
  const [profileStatus, setProfileStatus] = useState<VoiceProfileStatus>({
    is_enrolled: false,
    active_samples_count: 0,
  });

  // Guided Multi-Sampling State (5 Steps)
  const [currentStep, setCurrentStep] = useState<number>(1);
  const [isReadingMode, setIsReadingMode] = useState<boolean>(false);
  // Captured PCM stays in memory only (never persisted) so the user can hear the take before it is sent.
  const [completedSteps, setCompletedSteps] = useState<{ [step: number]: { duration: number; captured?: CapturedPcm } }>({});
  const [captureError, setCaptureError] = useState<string | null>(null);
  const [feedbackMessage, setFeedbackMessage] = useState<string | null>(null);

  // Active Tab: 'samples' (Galeria Cumulativa) | 'intake' (Sugestões de Chamadas)
  const [activeTab, setActiveTab] = useState<'samples' | 'intake'>('samples');

  // Samples and budget come from the service only; takes too (empty list when none).
  const [sampleList, setSampleList] = useState<SampleList | null>(null);
  const [samplesLoadFailed, setSamplesLoadFailed] = useState<boolean>(false);
  const [callTakes, setCallTakes] = useState<CallSuggestionTake[]>([]);
  const [deletingId, setDeletingId] = useState<string | null>(null);

  // Service job feedback: stage while running, quality when done, error/budget prompt when not.
  const [currentJob, setCurrentJob] = useState<EnrollmentJob | null>(null);
  const [jobBusy, setJobBusy] = useState<boolean>(false);
  const [enrollErrorText, setEnrollErrorText] = useState<string | null>(null);
  const [budgetError, setBudgetError] = useState<{ remainingSeconds: number | null } | null>(null);

  const labels = useMemo(() => buildEnrollmentLabels(t), [t]);
  const bannerLabels = useMemo(
    () => ({ ...labels, budgetExceededBody: interpolateBudgetBody(labels.budgetExceededBody, budgetError?.remainingSeconds ?? null, locale) }),
    [labels, budgetError, locale],
  );
  const samples = sampleList?.samples ?? [];

  // Recording State (both for guided steps and modal voluntary sample)
  const [isRecording, setIsRecording] = useState<boolean>(false);
  const [recordingElapsedSeconds, setRecordingElapsedSeconds] = useState<number>(0);
  const [liveVoiceLevel, setLiveVoiceLevel] = useState<number>(0);

  // Modal for "+ Adicionar Nova Amostra"
  const [isModalOpen, setIsModalOpen] = useState<boolean>(false);
  const [modalSampleName, setModalSampleName] = useState<string>('');
  const [modalCaptured, setModalCaptured] = useState<CapturedPcm | null>(null);

  // Audio Playback State
  const [playingAudioId, setPlayingAudioId] = useState<string | null>(null);
  const audioElementRef = useRef<HTMLAudioElement | null>(null);

  // Raw PCM capture (physical microphone only, no fallback) and in-memory preview playback.
  const recorderRef = useRef<PcmRecorder | null>(null);
  const deviceRef = useRef<DeviceInfo | null>(null);
  const streamRef = useRef<MediaStream | null>(null);
  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const recordingStartTimeRef = useRef<number>(0);
  const playbackCtxRef = useRef<AudioContext | null>(null);

  // Step Question metadata definition
  const stepQuestions = [
    {
      step: 1,
      categoryKey: 'voiceProfile.question1Category',
      textKey: 'voiceProfile.question1Text',
      fallbackKey: 'voiceProfile.question1Fallback',
    },
    {
      step: 2,
      categoryKey: 'voiceProfile.question2Category',
      textKey: 'voiceProfile.question2Text',
      fallbackKey: 'voiceProfile.question2Fallback',
    },
    {
      step: 3,
      categoryKey: 'voiceProfile.question3Category',
      textKey: 'voiceProfile.question3Text',
      fallbackKey: 'voiceProfile.question3Fallback',
    },
    {
      step: 4,
      categoryKey: 'voiceProfile.question4Category',
      textKey: 'voiceProfile.question4Text',
      fallbackKey: 'voiceProfile.question4Fallback',
    },
    {
      step: 5,
      categoryKey: 'voiceProfile.question5Category',
      textKey: 'voiceProfile.question5Text',
      fallbackKey: 'voiceProfile.question5Fallback',
    },
  ];

  const refreshSamples = useCallback(async (): Promise<SampleList | null> => {
    try {
      const res = await listSamples();
      if (enrollmentErrorCode(res) !== null || !('samples' in res)) {
        setSamplesLoadFailed(true);
        return null;
      }
      setSampleList(res);
      setSamplesLoadFailed(false);
      return res;
    } catch {
      setSamplesLoadFailed(true);
      return null;
    }
  }, []);

  const refreshCallTakes = useCallback(async () => {
    try {
      const res = await invokeBridge<{ takes?: CallSuggestionTake[] } | CallSuggestionTake[]>('get_call_takes');
      if (Array.isArray(res)) setCallTakes(res);
      else if (res && typeof res === 'object' && Array.isArray(res.takes)) setCallTakes(res.takes);
      else setCallTakes([]);
    } catch {
      setCallTakes([]);
    }
  }, []);

  // Initial state: samples, takes and profile status from the service.
  useEffect(() => {
    const initVoiceData = async () => {
      try {
        setIsReadingMode(localStorage.getItem(STORAGE_READING_MODE_KEY) === 'true');
      } catch {
        // UI preference only
      }

      const list = await refreshSamples();
      await refreshCallTakes();

      let initialProfile: VoiceProfileStatus = {
        is_enrolled: false,
        active_samples_count: list?.samples.length ?? 0,
      };
      try {
        const profileRes = await invokeBridge<unknown>('get_voice_profile');
        initialProfile = mergeVoiceProfileStatus(initialProfile, profileRes, list?.samples.length ?? 0);
      } catch {
        // service unreachable: status stays neutral
      }

      setProfileStatus(initialProfile);
      if (initialProfile.is_enrolled) {
        setCurrentStep(5);
      }
    };

    initVoiceData();
  }, [refreshSamples, refreshCallTakes]);

  /** Applies the outcome of a sample job; the budget error opens the gallery with delete highlighted. */
  const applyJobOutcome = (outcome: JobOutcome) => {
    if (outcome.kind === 'done') return;
    setCurrentJob(null);
    if (outcome.kind === 'show-budget-error') {
      setBudgetError({ remainingSeconds: outcome.remainingSeconds });
      if (shouldOpenGalleryOnError('ENROLL_BUDGET_EXCEEDED')) setActiveTab('samples');
      return;
    }
    if (shouldOpenGalleryOnError(outcome.code)) setActiveTab('samples');
    setEnrollErrorText(errorLabel(outcome.code, labels));
  };

  /** Sends one captured sample to the service and follows its job until done/failed. */
  const submitSample = async (captured: CapturedPcm, name: string): Promise<JobOutcome> => {
    setBudgetError(null);
    setEnrollErrorText(null);
    setCurrentJob(null);
    setJobBusy(true);
    let outcome: JobOutcome;
    try {
      const start = await addSample(captured, name);
      const startError = enrollmentErrorCode(start);
      if (startError !== null || !('jobId' in start)) {
        outcome = isBudgetError(startError)
          ? { kind: 'show-budget-error', remainingSeconds: null }
          : { kind: 'show-error', code: startError ?? 'ENROLL_FAILED' };
      } else {
        const job = await waitForJob(start.jobId, { onUpdate: setCurrentJob });
        setCurrentJob(job);
        outcome = nextStepAfterJob(job);
      }
      applyJobOutcome(outcome);
    } catch (err) {
      outcome = { kind: 'show-error', code: 'SERVICE_UNAVAILABLE' };
      setCurrentJob(null);
      setEnrollErrorText(err instanceof Error && err.message === 'timeout' ? t('voiceProfile.jobTimeout') : errorLabel('SERVICE_UNAVAILABLE', labels));
    } finally {
      setJobBusy(false);
    }
    await refreshSamples();
    return outcome;
  };

  // Stops any capture in progress and discards its PCM.
  const cleanupRecording = useCallback(() => {
    if (timerRef.current) {
      clearInterval(timerRef.current);
      timerRef.current = null;
    }
    const recorder = recorderRef.current;
    const device = deviceRef.current;
    recorderRef.current = null;
    deviceRef.current = null;
    if (recorder && device) recorder.stop(device).catch(() => undefined);
    streamRef.current?.getTracks().forEach((track) => track.stop());
    streamRef.current = null;
    setIsRecording(false);
    setLiveVoiceLevel(0);
  }, []);

  const stopPlayback = useCallback(() => {
    if (audioElementRef.current) {
      audioElementRef.current.pause();
      audioElementRef.current = null;
    }
    if (playbackCtxRef.current && playbackCtxRef.current.state !== 'closed') {
      playbackCtxRef.current.close().catch(() => undefined);
    }
    playbackCtxRef.current = null;
  }, []);

  useEffect(() => {
    return () => {
      cleanupRecording();
      stopPlayback();
    };
  }, [cleanupRecording, stopPlayback]);

  /**
   * Opens the raw physical microphone and starts the PCM recorder. If no physical microphone
   * opens, shows the error and does NOT start recording (spec D1: no fallback).
   */
  const startCapture = async (onLimit: () => void): Promise<boolean> => {
    cleanupRecording();
    setCaptureError(null);
    let acquired: { stream: MediaStream; device: DeviceInfo };
    try {
      acquired = await acquireRawPhysicalStream(selectedInputId, inputDevices);
    } catch (err) {
      if (!(err instanceof PhysicalMicUnavailableError)) console.warn('Physical microphone capture failed');
      setCaptureError(t('voiceProfile.physicalMicUnavailable'));
      return false;
    }
    streamRef.current = acquired.stream;
    const recorder = new PcmRecorder({ maxSeconds: MAX_RECORDING_SECONDS });
    try {
      await recorder.start(acquired.stream, (level01) => setLiveVoiceLevel(Math.round(level01 * 100)));
    } catch {
      acquired.stream.getTracks().forEach((track) => track.stop());
      streamRef.current = null;
      setCaptureError(t('voiceProfile.physicalMicUnavailable'));
      return false;
    }
    recorderRef.current = recorder;
    deviceRef.current = acquired.device;
    recordingStartTimeRef.current = Date.now();
    setRecordingElapsedSeconds(0);
    setIsRecording(true);
    const timer = setInterval(() => {
      const elapsed = (Date.now() - recordingStartTimeRef.current) / 1000;
      setRecordingElapsedSeconds(Math.round(elapsed * 10) / 10);
      if (elapsed >= MAX_RECORDING_SECONDS) {
        clearInterval(timer);
        onLimit();
      }
    }, 100);
    timerRef.current = timer;
    return true;
  };

  /** Stops the recorder and returns the captured PCM (null if nothing was recording). */
  const stopCapture = async (): Promise<CapturedPcm | null> => {
    if (timerRef.current) {
      clearInterval(timerRef.current);
      timerRef.current = null;
    }
    const recorder = recorderRef.current;
    const device = deviceRef.current;
    recorderRef.current = null;
    deviceRef.current = null;
    streamRef.current = null;
    setIsRecording(false);
    setLiveVoiceLevel(0);
    if (!recorder || !device) return null;
    try {
      return await recorder.stop(device);
    } catch {
      return null;
    }
  };

  // Plays a captured take from memory (no blob URL, nothing written anywhere).
  const playCaptured = (id: string, captured: CapturedPcm) => {
    const wasPlaying = playingAudioId === id;
    stopPlayback();
    setPlayingAudioId(null);
    if (wasPlaying || captured.pcm.length === 0) return;
    try {
      const ctx = new AudioContext({ sampleRate: captured.sampleRate });
      const buffer = ctx.createBuffer(1, captured.pcm.length, captured.sampleRate);
      buffer.getChannelData(0).set(captured.pcm);
      const src = ctx.createBufferSource();
      src.buffer = buffer;
      src.connect(ctx.destination);
      src.onended = () => {
        if (playbackCtxRef.current === ctx) stopPlayback();
        setPlayingAudioId((cur) => (cur === id ? null : cur));
      };
      playbackCtxRef.current = ctx;
      setPlayingAudioId(id);
      src.start();
    } catch {
      setPlayingAudioId(null);
    }
  };

  // Start Guided Step Recording (up to MAX_RECORDING_SECONDS with manual stop)
  const handleStartStepRecording = async (stepNum: number) => {
    await startCapture(() => {
      void finishStepRecording(stepNum);
    });
  };

  const finishStepRecording = async (stepNum: number) => {
    const captured = await stopCapture();
    if (!captured) return;

    const outcome = await submitSample(captured, t(stepQuestions[stepNum - 1].categoryKey));
    if (outcome.kind !== 'done') return;
    setCompletedSteps((prev) => ({
      ...prev,
      [stepNum]: { duration: outcome.quality?.speechSeconds ?? captured.durationSec, captured },
    }));

    setFeedbackMessage(t('voiceProfile.sampleCompleted'));
    setTimeout(() => setFeedbackMessage(null), 3500);
  };

  // Advance to next step
  const handleNextStep = () => {
    if (currentStep < 5) {
      setCurrentStep(currentStep + 1);
    }
  };

  // Re-record step
  const handleRedoStep = (stepNum: number) => {
    setCompletedSteps((prev) => {
      const copy = { ...prev };
      delete copy[stepNum];
      return copy;
    });
    handleStartStepRecording(stepNum);
  };

  // Toggle mode: Open questions vs Neutral reading
  const handleToggleReadingMode = () => {
    const nextVal = !isReadingMode;
    setIsReadingMode(nextVal);
    try {
      localStorage.setItem(STORAGE_READING_MODE_KEY, String(nextVal));
    } catch {}
  };

  /**
   * Builds the profile in the service (after the guided flow or from "Refazer perfil") and follows
   * the job. The status shown afterwards is the merged service status, never a local guess.
   */
  const handleBuildProfile = async () => {
    setBudgetError(null);
    setEnrollErrorText(null);
    setCurrentJob(null);
    setJobBusy(true);
    let done = false;
    try {
      const start = await buildProfile(t('voiceProfile.defaultProfileName'));
      const startError = enrollmentErrorCode(start);
      if (startError !== null || !('jobId' in start)) {
        setEnrollErrorText(errorLabel(startError ?? 'ENROLL_FAILED', labels));
      } else {
        const job = await waitForJob(start.jobId, { onUpdate: setCurrentJob });
        setCurrentJob(job);
        const outcome = nextStepAfterJob(job);
        if (outcome.kind === 'done') done = true;
        else applyJobOutcome(outcome);
      }
    } catch (err) {
      setCurrentJob(null);
      setEnrollErrorText(err instanceof Error && err.message === 'timeout' ? t('voiceProfile.jobTimeout') : errorLabel('SERVICE_UNAVAILABLE', labels));
    } finally {
      setJobBusy(false);
    }

    const list = await refreshSamples();
    let serviceStatus: VoiceProfileStatus | null = null;
    try {
      const res = await invokeBridge<unknown>('get_voice_profile');
      serviceStatus = normalizeVoiceProfileStatus(res);
      setProfileStatus((prev) => mergeVoiceProfileStatus(prev, res, list?.samples.length ?? 0));
    } catch {
      // the status keeps what the service last reported
    }
    if (done) {
      if (typeof window !== 'undefined' && serviceStatus) {
        window.dispatchEvent(new CustomEvent('clearcore_profile_updated', { detail: serviceStatus }));
      }
      setFeedbackMessage(t('voiceProfile.profileActivatedSuccess'));
      setTimeout(() => setFeedbackMessage(null), 5000);
    }
  };

  // Full Re-enrollment reset
  const handleResetEnrollment = () => {
    setCompletedSteps({});
    setCurrentStep(1);
  };

  // Playback of audio the service points to (call takes). Nothing is synthesized when absent.
  const handlePlayAudio = (id: string, audioUrl?: string) => {
    const wasPlaying = playingAudioId === id;
    stopPlayback();
    setPlayingAudioId(null);
    if (wasPlaying || !audioUrl) return;

    const audio = new Audio(audioUrl);
    audioElementRef.current = audio;
    setPlayingAudioId(id);
    audio.onended = () => setPlayingAudioId(null);
    audio.onerror = () => setPlayingAudioId(null);
    audio.play().catch(() => setPlayingAudioId(null));
  };

  // Sends the local profile (never service keys) and applies the fresh service status from the reply.
  const pushProfileStatus = (local: VoiceProfileStatus) => {
    invokeBridge<unknown>('set_voice_profile', { profile: stripServiceVoiceProfileKeys(local) })
      .catch(() => undefined)
      .then((res) => setProfileStatus((prev) => applySetVoiceProfileResult(prev, local, res)));
  };

  // Main-process pushes (same merged payload as the set_voice_profile reply).
  useEffect(() => {
    const api = typeof window !== 'undefined' ? window.clearcoreApi : undefined;
    if (!api?.onVoiceProfileUpdate) return;
    return api.onVoiceProfileUpdate((profile) => {
      setProfileStatus((prev) => applySetVoiceProfileResult(prev, prev, profile));
    });
  }, []);

  // Delete Sample from the service gallery
  const handleDeleteSample = async (id: string) => {
    setDeletingId(id);
    try {
      await deleteSample(id);
    } catch {
      // the refreshed list shows what the service still has
    }
    const list = await refreshSamples();
    setDeletingId(null);
    if (list) setBudgetError(null);

    const remaining = list?.samples.length ?? samples.length;
    const newStatus: VoiceProfileStatus = {
      ...stripServiceVoiceProfileKeys(profileStatus),
      is_enrolled: remaining > 0,
      active_samples_count: remaining,
    };
    pushProfileStatus(newStatus);
  };

  // Approve Call Suggestion Take: the service rechecks the speech budget.
  const handleApproveCallTake = async (take: CallSuggestionTake) => {
    setBudgetError(null);
    setEnrollErrorText(null);
    let res: unknown;
    try {
      res = await invokeBridge<unknown>('approve_call_take', { id: take.id, name: take.title });
    } catch {
      res = { errorCode: 'SERVICE_UNAVAILABLE' };
    }
    if (shouldShowTakeError(res)) {
      const code = enrollmentErrorCode(res);
      if (isBudgetError(code)) {
        const remaining = (res as { remainingSeconds?: unknown }).remainingSeconds;
        setBudgetError({ remainingSeconds: typeof remaining === 'number' ? remaining : null });
        setActiveTab('samples');
      } else {
        setEnrollErrorText(errorLabel(code, labels));
      }
    } else {
      setFeedbackMessage(t('voiceProfile.takeApprovedFeedback'));
      setTimeout(() => setFeedbackMessage(null), 4000);
    }
    await Promise.all([refreshSamples(), refreshCallTakes()]);
  };

  // Dismiss Call Suggestion Take
  const handleDismissCallTake = async (id: string) => {
    await invokeBridge('dismiss_call_take', { id }).catch(() => undefined);
    await refreshCallTakes();
    setFeedbackMessage(t('voiceProfile.takeDismissedFeedback'));
    setTimeout(() => setFeedbackMessage(null), 3000);
  };

  const finishModalRecording = async () => {
    const captured = await stopCapture();
    if (captured) setModalCaptured(captured);
  };

  // Modal: Start Recording voluntary sample
  const handleStartModalRecording = async () => {
    await startCapture(() => {
      void finishModalRecording();
    });
  };

  // Modal: send the voluntary sample to the service
  const handleSaveModalSample = async () => {
    if (!modalCaptured) return;
    const name = modalSampleName.trim() || t('voiceProfile.defaultSampleName', { n: String(samples.length + 1) });
    const outcome = await submitSample(modalCaptured, name);
    if (outcome.kind === 'show-error') return; // the modal stays open with the error
    // Done, or budget exceeded: close the modal so the gallery (and its delete buttons) is reachable.
    setIsModalOpen(false);
    if (outcome.kind === 'done') {
      setModalSampleName('');
      setModalCaptured(null);
      setFeedbackMessage(t('voiceProfile.sampleCompleted'));
      setTimeout(() => setFeedbackMessage(null), 3500);
    }
  };

  const completedCount = Object.keys(completedSteps).length;
  const isAllStepsCompleted = completedCount >= 5;
  const currentQ = stepQuestions[currentStep - 1];


  const statusLabelKey = voiceProfileStatusLabelKey(profileStatus);
  const statusLabelActive = statusLabelKey === 'active';
  const statusLabel =
    statusLabelKey === 'active'
      ? t('voiceProfile.statusActive')
      : statusLabelKey === 'storedNotApplied'
        ? t('voiceProfile.storedNotApplied')
        : statusLabelKey === 'enrolledUnconfirmed'
          ? t('voiceProfile.enrolledUnconfirmed')
          : t('voiceProfile.statusPending');

  return (
    <div className="card voice-profile-card">
      {/* Header & Status Section */}
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start', flexWrap: 'wrap', gap: 12, marginBottom: 16 }}>
        <div>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8, flexWrap: 'wrap' }}>
            <h2 className="card-title" style={{ margin: 0 }}>{t('voiceProfile.title')}</h2>
            <span className="profile-badge-ecapa">
              {t('voiceProfile.badge')}
            </span>
            <span className={`status-pill ${statusLabelActive ? 'pill-active' : 'pill-pending'}`}>
              {statusLabel}
            </span>
          </div>
          <p style={{ color: 'var(--text-muted)', fontSize: '0.85rem', marginTop: 6, lineHeight: 1.45 }}>
            {t('voiceProfile.description')}
          </p>
        </div>

        <div style={{ display: 'flex', gap: 8, alignItems: 'center' }}>
          {samples.length > 0 && (
            <button
              className="action-btn"
              disabled={jobBusy || isRecording}
              onClick={() => void handleBuildProfile()}
              style={{ fontSize: '0.8rem', padding: '6px 12px' }}
            >
              {t('voiceProfile.rebuildProfileBtn')}
            </button>
          )}
          {profileStatus.is_enrolled && (
            <button
              className="action-btn"
              onClick={handleResetEnrollment}
              style={{ fontSize: '0.8rem', padding: '6px 12px' }}
            >
              {t('voiceProfile.reEnrollBtn')}
            </button>
          )}
        </div>
      </div>

      <div style={{ marginBottom: 12 }}>
        <DevModelNotice labels={labels} />
      </div>

      {feedbackMessage && (
        <div className="feedback-banner success-banner">
          {feedbackMessage}
        </div>
      )}

      {/* Service job feedback (outside the modal) */}
      {!isModalOpen && (jobBusy || currentJob || enrollErrorText) && (
        <div style={{ marginBottom: 12 }}>
          {jobBusy && !currentJob && (
            <div role="status" style={{ fontSize: 13, color: 'var(--text-muted)' }}>{t('voiceProfile.sendingSample')}</div>
          )}
          <EnrollmentJobStatus job={currentJob} labels={labels} />
          {enrollErrorText && (
            <div role="alert" style={{ color: '#f87171', fontSize: 13, marginTop: 4 }}>{enrollErrorText}</div>
          )}
        </div>
      )}

      {/* Profile Overview Bar */}
      <div className="profile-overview-box">
        <div className="overview-metric">
          <div className="overview-label">{t('voiceProfile.statusTitle')}</div>
          <div className="overview-value" style={{ color: statusLabelActive ? '#4ade80' : '#fbbf24' }}>
            {statusLabel}
          </div>
          {statusLabelKey === 'active' && (
            <div style={{ color: 'var(--text-muted)', fontSize: '0.75rem', marginTop: 4 }}>
              {t('voiceProfile.appliedInServiceNote')}
            </div>
          )}
          {profileStatus.voice_profile_error && statusLabelKey !== 'active' && (
            <div style={{ color: '#fbbf24', fontSize: '0.75rem', marginTop: 4 }}>
              {t(`voiceProfile.${voiceProfileErrorKey(profileStatus.voice_profile_error)}`)}
            </div>
          )}
        </div>
        <div className="overview-metric">
          <div className="overview-label">{t('voiceProfile.samplesRegisteredTitle')}</div>
          <div className="overview-value">
            {t('voiceProfile.statusSamplesPill', { count: String(sampleList ? samples.length : profileStatus.active_samples_count) })}
          </div>
        </div>
        <div className="overview-metric">
          <div className="overview-label">{t('voiceProfile.neuralEqStatusTitle')}</div>
          <div className="overview-value" style={{ color: profileStatus.neural_eq_calibrated ? '#4ade80' : 'var(--text-muted)' }}>
            {profileStatus.neural_eq_calibrated ? t('voiceProfile.neuralEqCalibrated') : t('voiceProfile.neuralEqPending')}
          </div>
        </div>
      </div>

      {/* Guided Multi-Sampling Stepper (If not enrolled or re-enrolling) */}
      {(!profileStatus.is_enrolled || completedCount < 5) && (
        <div className="guided-sampling-section">
          {/* Progress Header */}
          <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 10, flexWrap: 'wrap', gap: 8 }}>
            <div style={{ fontWeight: 600, fontSize: '0.95rem', color: '#93c5fd' }}>
              {t('voiceProfile.stepProgress', { current: String(currentStep), total: '5' })}: {t(currentQ.categoryKey)}
            </div>
            <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
              <span style={{ fontSize: '0.8rem', color: 'var(--text-muted)' }}>
                {t('voiceProfile.stepPercent', { percent: String(Math.round((completedCount / 5) * 100)) })}
              </span>
              <button
                className="mode-toggle-link"
                onClick={handleToggleReadingMode}
                title={isReadingMode ? t('voiceProfile.toggleModeBack') : t('voiceProfile.toggleModePrompt')}
              >
                {isReadingMode ? t('voiceProfile.toggleModeBack') : t('voiceProfile.toggleModePrompt')}
              </button>
            </div>
          </div>

          {/* Stepper Progress Bar (1/5 to 5/5) */}
          <div className="stepper-bar-container">
            {[1, 2, 3, 4, 5].map((stepIdx) => {
              const isDone = Boolean(completedSteps[stepIdx]);
              const isCur = currentStep === stepIdx;
              return (
                <div
                  key={stepIdx}
                  className={`stepper-segment ${isDone ? 'done' : isCur ? 'current' : 'pending'}`}
                  onClick={() => setCurrentStep(stepIdx)}
                  title={t('voiceProfile.stepTitle', { n: String(stepIdx) })}
                >
                  <div className="segment-number">{isDone ? '✓' : stepIdx}</div>
                  <div className="segment-fill" />
                </div>
              );
            })}
          </div>

          {/* Prompt Box */}
          <div className="prompt-display-card">
            <div className="prompt-mode-tag">
              {isReadingMode ? `📖 ${t('voiceProfile.readingMode')}` : `💬 ${t('voiceProfile.openQuestionsMode')}`}
            </div>
            <div className="prompt-main-text">
              {isReadingMode ? `"${t(currentQ.fallbackKey)}"` : `"${t(currentQ.textKey)}"`}
            </div>
            <div className="prompt-hint-sub">
              {isReadingMode ? t('voiceProfile.readingHint') : t('voiceProfile.openHint')}
            </div>
          </div>

          {/* Live Dynamic VU Meter */}
          <div className="vu-meter-panel">
            <div style={{ display: 'flex', justifyContent: 'space-between', fontSize: '0.8rem', marginBottom: 4, color: 'var(--text-muted)' }}>
              <span>{t('voiceProfile.voiceLevel')}</span>
              <span>{isRecording ? `${liveVoiceLevel}%` : '0%'}</span>
            </div>
            <div className="vu-meter-track">
              <div
                className="vu-meter-bar"
                style={{
                  width: `${isRecording ? liveVoiceLevel : 0}%`,
                  background:
                    liveVoiceLevel > 80
                      ? 'linear-gradient(90deg, #22c55e, #eab308, #ef4444)'
                      : liveVoiceLevel > 40
                      ? 'linear-gradient(90deg, #22c55e, #38bdf8)'
                      : '#22c55e',
                }}
              />
            </div>
          </div>

          {captureError && !isModalOpen && (
            <div role="alert" className="feedback-banner" style={{ color: '#f87171', marginBottom: 10 }}>
              {captureError}
            </div>
          )}

          {/* Action Row for the Step */}
          <div className="stepper-action-row">
            {!isRecording ? (
              completedSteps[currentStep] ? (
                <div style={{ display: 'flex', gap: 10, alignItems: 'center', flexWrap: 'wrap' }}>
                  <button
                    className="action-btn play-sample-btn"
                    disabled={!completedSteps[currentStep].captured}
                    onClick={() => {
                      const c = completedSteps[currentStep].captured;
                      if (c) playCaptured(`step-${currentStep}`, c);
                    }}
                  >
                    {playingAudioId === `step-${currentStep}` ? t('voiceProfile.stopSample') : t('voiceProfile.playSample')}
                  </button>
                  <button
                    className="action-btn"
                    disabled={jobBusy}
                    onClick={() => handleRedoStep(currentStep)}
                  >
                    {t('voiceProfile.redoSample')}
                  </button>
                  {currentStep < 5 && (
                    <button
                      className="action-btn primary-next-btn"
                      onClick={handleNextStep}
                    >
                      {t('voiceProfile.nextStep')}
                    </button>
                  )}
                </div>
              ) : (
                <button
                  className="record-btn-trigger"
                  disabled={jobBusy}
                  onClick={() => handleStartStepRecording(currentStep)}
                >
                  🎙️ {t('voiceProfile.recordSample')}
                </button>
              )
            ) : (
              <div className="recording-active-container">
                <div className="recording-status-group">
                  <span className="recording-pulsing-dot" />
                  <span style={{ fontWeight: 600, color: '#f87171' }}>
                    {t('voiceProfile.recordingStatus', {
                      elapsed: formatSecondsForLocale(recordingElapsedSeconds, locale),
                      max: String(MAX_RECORDING_SECONDS),
                    })}
                  </span>
                  <span style={{ fontSize: '0.8rem', color: 'var(--text-muted)' }}>
                    ({t('voiceProfile.recordingHint')})
                  </span>
                </div>
                <button
                  type="button"
                  className="stop-record-btn"
                  disabled={recordingElapsedSeconds < MIN_RECORDING_SECONDS}
                  onClick={() => finishStepRecording(currentStep)}
                  title={
                    recordingElapsedSeconds < MIN_RECORDING_SECONDS
                      ? t('voiceProfile.minRecordingTitle', { min: formatSecondsForLocale(MIN_RECORDING_SECONDS, locale) })
                      : t('voiceProfile.stopRecordingBtn')
                  }
                >
                  ⏹️ {t('voiceProfile.stopRecordingBtn')}
                </button>
              </div>
            )}

            {isAllStepsCompleted && (
              <button
                className="activate-profile-master-btn"
                disabled={jobBusy || isRecording}
                onClick={() => void handleBuildProfile()}
              >
                {t('voiceProfile.activateProfileBtn')}
              </button>
            )}
          </div>
        </div>
      )}

      {/* Tabs Header: Cumulative Gallery vs Call Suggestions */}
      <div className="profile-subtabs-nav">
        <button
          className={`subtab-btn ${activeTab === 'samples' ? 'subtab-active' : ''}`}
          onClick={() => setActiveTab('samples')}
        >
          {t('voiceProfile.tabSamples', { count: String(samples.length) })}
        </button>
        <button
          className={`subtab-btn ${activeTab === 'intake' ? 'subtab-active' : ''}`}
          onClick={() => setActiveTab('intake')}
        >
          {t('voiceProfile.tabCallSuggestions', { count: String(callTakes.length) })}
          {callTakes.length > 0 && <span className="tab-count-badge">{callTakes.length}</span>}
        </button>
      </div>

      {/* Tab 1: Cumulative Samples Gallery */}
      {activeTab === 'samples' && (
        <div className="tab-pane-content">
          <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 12, flexWrap: 'wrap', gap: 8 }}>
            <p style={{ color: 'var(--text-muted)', fontSize: '0.85rem', margin: 0 }}>
              {t('voiceProfile.sampleGalleryDesc')}
            </p>
            <button
              className="action-btn add-sample-accent-btn"
              onClick={() => {
                setEnrollErrorText(null);
                setCurrentJob(null);
                setIsModalOpen(true);
              }}
            >
              {t('voiceProfile.addNewSampleBtn')}
            </button>
          </div>

          {sampleList && (
            <div style={{ marginBottom: 12 }}>
              <VoiceBudgetMeter budget={sampleList.budget} labels={labels} />
            </div>
          )}

          {budgetError && (
            <div style={{ marginBottom: 12 }}>
              <BudgetErrorBanner remainingSeconds={budgetError.remainingSeconds} labels={bannerLabels} />
            </div>
          )}

          {samplesLoadFailed && (
            <div role="alert" style={{ color: '#fbbf24', fontSize: 13, marginBottom: 12 }}>
              {t('voiceProfile.samplesLoadFailed')}
            </div>
          )}

          {sampleList && samples.length > 0 ? (
            <VoiceSampleGallery
              samples={samples}
              budget={sampleList.budget}
              labels={labels}
              onDelete={(id) => void handleDeleteSample(id)}
              deletingId={deletingId}
              highlightDelete={budgetError !== null}
            />
          ) : (
            <div className="empty-state-card">
              {t('voiceProfile.emptyGallery')}
            </div>
          )}
        </div>
      )}

      {/* Tab 2: Call Suggestions (Voice Intake Engine) */}
      {activeTab === 'intake' && (
        <div className="tab-pane-content">
          <div style={{ marginBottom: 14 }}>
            <p style={{ color: 'var(--text-muted)', fontSize: '0.85rem', margin: 0, lineHeight: 1.45 }}>
              {t('voiceProfile.callSuggestionsDesc')}
            </p>
          </div>

          {callTakes.length > 0 ? (
            <div className="intake-takes-list">
              {callTakes.map((take) => (
                <div key={take.id} className="intake-take-card">
                  <div className="intake-take-info">
                    <div className="take-title-row">
                      <span className="take-badge-live">{t('voiceProfile.takeBadge')}</span>
                      {take.title && <strong style={{ fontSize: '0.95rem' }}>{take.title}</strong>}
                    </div>
                    <div className="take-meta-row">
                      <span>🕒 {take.timestamp}</span>
                      {typeof take.speech_seconds === 'number' && (
                        <span>⏱ {t('voiceProfile.takeSpeech', { sec: formatSecondsForLocale(take.speech_seconds, locale) })}</span>
                      )}
                      {typeof take.speech_seconds !== 'number' && typeof take.durationSec === 'number' && (
                        <span>⏱ {t('voiceProfile.takeDuration', { sec: formatSecondsForLocale(take.durationSec, locale) })}</span>
                      )}
                      {typeof take.snrDb === 'number' && (
                        <span className="take-snr-badge">{t('voiceProfile.takeSnr', { snr: formatSecondsForLocale(take.snrDb, locale) })}</span>
                      )}
                      {take.device_label && <span>🎙️ {take.device_label}</span>}
                    </div>
                  </div>

                  <div className="intake-take-actions">
                    {take.audioUrl && (
                      <button
                        className="action-btn take-play-btn"
                        onClick={() => handlePlayAudio(take.id, take.audioUrl)}
                      >
                        {playingAudioId === take.id ? t('voiceProfile.stopSample') : t('voiceProfile.playSample')}
                      </button>
                    )}
                    <button
                      className="action-btn take-approve-btn"
                      onClick={() => void handleApproveCallTake(take)}
                    >
                      {t('voiceProfile.approveTake')}
                    </button>
                    <button
                      className="action-btn take-dismiss-btn"
                      onClick={() => void handleDismissCallTake(take.id)}
                    >
                      {t('voiceProfile.dismissTake')}
                    </button>
                  </div>
                </div>
              ))}
            </div>
          ) : (
            <div className="empty-state-card">
              {t('voiceProfile.emptyCallSuggestions')}
            </div>
          )}
        </div>
      )}

      {/* Modal: "+ Adicionar Nova Amostra" */}
      {isModalOpen && (
        <div className="modal-overlay">
          <div className="modal-content profile-add-modal">
            <h3 style={{ fontSize: '1.2rem', marginBottom: 8, color: 'var(--text-main)' }}>
              {t('voiceProfile.modalAddSampleTitle')}
            </h3>
            <p style={{ color: 'var(--text-muted)', fontSize: '0.85rem', marginBottom: 16 }}>
              {t('voiceProfile.modalAddSampleDesc')}
            </p>

            <div style={{ marginBottom: 14 }}>
              <label style={{ display: 'block', fontSize: '0.8rem', color: 'var(--text-muted)', marginBottom: 6 }}>
                {t('voiceProfile.sampleNameLabel')}
              </label>
              <input
                type="text"
                className="device-select"
                placeholder={t('voiceProfile.sampleNamePlaceholder')}
                value={modalSampleName}
                onChange={(e) => setModalSampleName(e.target.value)}
              />
            </div>

            {/* VU Meter inside modal */}
            <div className="vu-meter-panel" style={{ marginBottom: 16 }}>
              <div style={{ display: 'flex', justifyContent: 'space-between', fontSize: '0.8rem', marginBottom: 4, color: 'var(--text-muted)' }}>
                <span>{t('voiceProfile.voiceLevel')}</span>
                <span>{isRecording ? `${liveVoiceLevel}%` : '0%'}</span>
              </div>
              <div className="vu-meter-track">
                <div
                  className="vu-meter-bar"
                  style={{ width: `${isRecording ? liveVoiceLevel : 0}%`, background: '#22c55e' }}
                />
              </div>
            </div>

            {captureError && (
              <div role="alert" className="feedback-banner" style={{ color: '#f87171', marginBottom: 12 }}>
                {captureError}
              </div>
            )}

            <div style={{ display: 'flex', justifyContent: 'center', marginBottom: 16 }}>
              {!isRecording ? (
                <button
                  className="record-btn-trigger"
                  disabled={jobBusy}
                  onClick={handleStartModalRecording}
                >
                  🎙️ {modalCaptured ? t('voiceProfile.redoSample') : t('voiceProfile.recordSample')}
                </button>
              ) : (
                <div className="recording-active-container">
                  <div className="recording-status-group">
                    <span className="recording-pulsing-dot" />
                    <span style={{ fontWeight: 600, color: '#f87171' }}>
                      {t('voiceProfile.recordingStatus', {
                        elapsed: formatSecondsForLocale(recordingElapsedSeconds, locale),
                        max: String(MAX_RECORDING_SECONDS),
                      })}
                    </span>
                  </div>
                  <button
                    type="button"
                    className="stop-record-btn"
                    disabled={recordingElapsedSeconds < MIN_RECORDING_SECONDS}
                    onClick={finishModalRecording}
                    title={
                      recordingElapsedSeconds < MIN_RECORDING_SECONDS
                        ? t('voiceProfile.minRecordingTitle', { min: formatSecondsForLocale(MIN_RECORDING_SECONDS, locale) })
                        : t('voiceProfile.stopRecordingBtn')
                    }
                  >
                    ⏹️ {t('voiceProfile.stopRecordingBtn')}
                  </button>
                </div>
              )}
            </div>

            {modalCaptured && !isRecording && (
              <div style={{ display: 'flex', justifyContent: 'center', alignItems: 'center', gap: 10, marginBottom: 16 }}>
                <button
                  className="action-btn"
                  onClick={() => playCaptured('modal-preview', modalCaptured)}
                >
                  {playingAudioId === 'modal-preview' ? t('voiceProfile.stopSample') : t('voiceProfile.playSample')}
                </button>
                <span style={{ fontSize: '0.85rem', color: 'var(--text-muted)' }}>
                  {t('voiceProfile.recordedDuration', { sec: formatSecondsForLocale(modalCaptured.durationSec, locale) })}
                </span>
              </div>
            )}

            {(jobBusy || currentJob || enrollErrorText) && (
              <div style={{ marginBottom: 12 }}>
                {jobBusy && !currentJob && (
                  <div role="status" style={{ fontSize: 13, color: 'var(--text-muted)' }}>{t('voiceProfile.sendingSample')}</div>
                )}
                <EnrollmentJobStatus job={currentJob} labels={labels} />
                {enrollErrorText && (
                  <div role="alert" style={{ color: '#f87171', fontSize: 13, marginTop: 4 }}>{enrollErrorText}</div>
                )}
              </div>
            )}

            <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 10, marginTop: 12 }}>
              <button
                className="action-btn"
                onClick={() => {
                  cleanupRecording();
                  stopPlayback();
                  setIsModalOpen(false);
                  setModalCaptured(null);
                  setCaptureError(null);
                  setEnrollErrorText(null);
                }}
              >
                {t('voiceProfile.modalCancel')}
              </button>
              <button
                className="action-btn primary-next-btn"
                disabled={!modalCaptured || jobBusy || isRecording}
                onClick={() => void handleSaveModalSample()}
              >
                {t('voiceProfile.modalSave')}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
};
