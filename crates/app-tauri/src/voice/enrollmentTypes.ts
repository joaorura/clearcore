export const MAX_SPEECH_SECONDS = 90;
export const MIN_TAKE_MARGIN_SECONDS = 5;
export const CAPTURE_SAMPLE_RATE = 48_000;
export const MAX_RECORD_SECONDS = 30;                 // per sample, UI guard
export const MIN_RECORD_SECONDS = 1.5;

export type EnrollErrorCode =
  | 'ENROLL_CLIPPING' | 'ENROLL_TOO_QUIET' | 'ENROLL_TOO_LITTLE_SPEECH' | 'ENROLL_MODEL_NOT_CONFIGURED'
  | 'ENROLL_BUDGET_EXCEEDED' | 'ENROLL_INVALID_AUDIO' | 'ENROLL_PAYLOAD_TOO_LARGE'
  | 'ENROLL_JOB_NOT_FOUND' | 'ENROLL_BUSY' | 'ENROLL_FAILED' | 'SERVICE_UNAVAILABLE';

export interface DeviceInfo { label: string; idHash: string }          // idHash: 64 lowercase hex
export interface CapturedPcm {
  pcm: Float32Array; sampleRate: typeof CAPTURE_SAMPLE_RATE; durationSec: number;
  peak: number; rmsDbfs: number; device: DeviceInfo;
}
export interface SpeechBudget { usedSeconds: number; maxSeconds: number; remainingSeconds: number }
export interface ServiceSample {
  id: string; name: string; timestamp: string; speechSeconds: number; deviceLabel: string;
  usedInProfile: boolean; needsReenroll: boolean; otherMicrophone: boolean;
}
export interface SampleList { samples: ServiceSample[]; budget: SpeechBudget }
export type JobState = 'running' | 'done' | 'failed';
export type JobStage = 'queued' | 'denoise' | 'trim' | 'eq' | 'enroll' | 'apply';
export interface Quality { peak: number; rmsDbfs: number; activeFraction: number; speechSeconds: number }
export interface EnrollmentJob {
  jobId: string; state: JobState; stage: JobStage; errorCode: EnrollErrorCode | null;
  remainingSeconds: number | null; sampleId: string | null; profileId: string | null; quality: Quality | null;
}

/** Renderer ⇄ main command names (all go through invokeBridge → ipcMain.handle). */
export const CMD = {
  addSample: 'enrollment_add_sample',       // { pcm: Float32Array, sampleRate: 48000, name: string, device: DeviceInfo } -> { jobId } | { errorCode }
  buildProfile: 'enrollment_build_profile', // { name: string } -> { jobId } | { errorCode }
  getJob: 'enrollment_get_job',             // { jobId: string } -> EnrollmentJob | { errorCode }
  listSamples: 'enrollment_list_samples',   // {} -> SampleList | { errorCode }
  deleteSample: 'enrollment_delete_sample', // { id: string } -> { success: boolean } | { errorCode }
} as const;

/** Every user-visible string the new components render; E2 fills it from i18n. */
export interface EnrollmentLabels {
  budgetTitle: string; budgetUsed: string; budgetRemaining: string; seconds: string;
  budgetExceededTitle: string; budgetExceededBody: string;        // body has "{remaining}" placeholder
  deleteAction: string; deleting: string;
  otherMicrophone: string; needsReenroll: string; usedInProfile: string; notUsed: string;
  stageQueued: string; stageDenoise: string; stageTrim: string; stageEq: string; stageEnroll: string; stageApply: string;
  jobDone: string; jobFailed: string;
  devModelNotice: string;                                          // "development model, not approved"
  qualityPeak: string; qualityLevel: string; qualitySpeech: string;
  errors: Record<EnrollErrorCode | 'UNKNOWN', string>;
}
