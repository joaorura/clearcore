import type { CapturedPcm } from '../enrollmentTypes';
import type { JobOutcome } from './voiceProfileLogic';

/** One accepted take of a guided step. The PCM stays in memory only (never persisted). */
export interface StepTake {
  duration: number;
  captured?: CapturedPcm;
  /** Service id of the sample this step stored, when the job reported it. */
  sampleId?: string | null;
}

export type CompletedSteps = { [step: number]: StepTake };

export const GUIDED_STEP_COUNT = 5;

export interface StepQuestion { step: number; categoryKey: string; textKey: string; fallbackKey: string }

export const STEP_QUESTIONS: StepQuestion[] = [1, 2, 3, 4, 5].map((step) => ({
  step,
  categoryKey: `voiceProfile.question${step}Category`,
  textKey: `voiceProfile.question${step}Text`,
  fallbackKey: `voiceProfile.question${step}Fallback`,
}));

/**
 * The step's take after a submission: the accepted sample (service speech seconds when known)
 * or, when the new one failed, whatever the step had before.
 */
export function stepAfterSubmit(
  prev: StepTake | undefined,
  outcome: JobOutcome,
  sampleId: string | null,
  captured: CapturedPcm,
): StepTake | undefined {
  if (outcome.kind !== 'done') return prev;
  return { duration: outcome.quality?.speechSeconds ?? captured.durationSec, captured, sampleId };
}

/**
 * Re-recording a guided step: the step's old sample is deleted in the service only AFTER the new
 * one was accepted. If the new one fails, the old one stays (returns null).
 */
export function replacedSampleToDelete(
  prev: Pick<StepTake, 'sampleId'> | undefined,
  outcome: JobOutcome,
  newSampleId: string | null,
): string | null {
  if (outcome.kind !== 'done') return null;
  const old = prev?.sampleId;
  if (typeof old !== 'string' || old.length === 0 || old === newSampleId) return null;
  return old;
}
