import type { CapturedPcm, ServiceSample } from '../enrollmentTypes';
import type { JobOutcome, Translate } from './voiceProfileLogic';

/**
 * One accepted take of a guided step. It keeps no PCM: the raw audio is zeroed once sent to the
 * service, so there is nothing left to play back.
 */
export interface StepTake {
  duration: number;
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
  return { duration: outcome.quality?.speechSeconds ?? captured.durationSec, sampleId };
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

/** Clicking a stepper segment: ignored while recording or submitting (the take belongs to the current step). */
export function stepAfterSelect(current: number, requested: number, isBusy: boolean): number {
  if (isBusy) return current;
  return Number.isInteger(requested) && requested >= 1 && requested <= GUIDED_STEP_COUNT ? requested : current;
}

/** Strips accents, punctuation, and excessive whitespace for resilient category comparison. */
export function normalizeCategoryName(name: string): string {
  return name
    .toLowerCase()
    .normalize('NFD')
    .replace(/[\u0300-\u036f]/g, '')
    .replace(/[^a-z0-9]/g, ' ')
    .trim()
    .replace(/\s+/g, ' ');
}

const KNOWN_STEP_CATEGORY_NAMES: Record<number, string[]> = {
  1: ['inicio de reuniao', 'meeting kickoff', 'reuniao', 'kickoff'],
  2: ['rotina matinal', 'morning routine', 'rotina', 'matinal'],
  3: ['foco de trabalho', 'work focus', 'foco', 'work'],
  4: ['espaco de trabalho', 'workspace setup', 'workspace', 'espaco'],
  5: ['lazer descontracao', 'leisure downtime', 'leisure and downtime', 'lazer', 'descontracao'],
};

/** Determines if a stored sample from the daemon belongs to a specific guided step. */
export function matchSampleToStep(sample: Pick<ServiceSample, 'name'>, step: number, t?: Translate): boolean {
  if (step < 1 || step > GUIDED_STEP_COUNT) return false;
  const normName = normalizeCategoryName(sample.name);
  if (!normName) return false;
  if (step === 3 && (normName.includes('espaco') || normName.includes('workspace'))) return false;

  // 1. Translated category check
  if (t) {
    const translated = t(STEP_QUESTIONS[step - 1].categoryKey);
    const normTranslated = normalizeCategoryName(translated);
    if (normTranslated && (normName === normTranslated || normName.includes(normTranslated) || normTranslated.includes(normName))) {
      return true;
    }
  }

  // 2. Known category names check (pt-BR / en-US)
  const known = KNOWN_STEP_CATEGORY_NAMES[step] ?? [];
  for (const k of known) {
    if (normName === k || normName.includes(k)) return true;
  }

  // 3. Step numbering in name (e.g. "Etapa 1", "Passo 1", "Step 1", "Amostra 1")
  const stepRegex = new RegExp(`\\b(?:etapa|passo|step|pergunta|question|amostra|sample)\\s*#?\\s*${step}\\b`, 'i');
  if (stepRegex.test(sample.name)) return true;

  if (new RegExp(`^#?\\s*${step}$`).test(sample.name.trim())) return true;

  return false;
}

/** Synchronizes wizard completedSteps with samples stored in the service daemon. */
export function syncCompletedStepsFromSamples(
  samples: ServiceSample[],
  prevSteps: CompletedSteps = {},
  t?: Translate,
  isEnrolled?: boolean,
): CompletedSteps {
  const result: CompletedSteps = { ...prevSteps };

  // Remove steps whose stored sample was deleted from the service
  for (let s = 1; s <= GUIDED_STEP_COUNT; s++) {
    const existing = result[s];
    if (existing?.sampleId) {
      const stillExists = samples.some((samp) => samp.id === existing.sampleId);
      if (!stillExists) {
        delete result[s];
      }
    }
  }

  // Populate missing steps from matching service samples
  for (let s = 1; s <= GUIDED_STEP_COUNT; s++) {
    if (result[s]?.sampleId) continue;

    const matching = samples.filter((samp) => matchSampleToStep(samp, s, t));
    if (matching.length > 0) {
      matching.sort((a, b) => {
        // Prioritize samples from current microphone
        if (!a.otherMicrophone && b.otherMicrophone) return -1;
        if (a.otherMicrophone && !b.otherMicrophone) return 1;
        // Prioritize newest sample
        const tsA = Number(a.timestamp) || 0;
        const tsB = Number(b.timestamp) || 0;
        return tsB - tsA;
      });
      const best = matching[0];
      result[s] = {
        duration: best.speechSeconds,
        sampleId: best.id,
      };
    }
  }

  // Fallback 1: fill remaining empty steps with any unused samples from the gallery
  const usedSampleIds = new Set(Object.values(result).map((take) => take.sampleId).filter(Boolean));
  const unusedSamples = samples.filter((samp) => !usedSampleIds.has(samp.id));
  let unusedIdx = 0;
  for (let s = 1; s <= GUIDED_STEP_COUNT && unusedIdx < unusedSamples.length; s++) {
    if (!result[s]?.sampleId) {
      const samp = unusedSamples[unusedIdx++];
      result[s] = {
        duration: samp.speechSeconds,
        sampleId: samp.id,
      };
    }
  }

  // Fallback 2: if profile is confirmed enrolled, ensure all 5 steps are marked complete
  if (isEnrolled) {
    for (let s = 1; s <= GUIDED_STEP_COUNT; s++) {
      if (!result[s]) {
        result[s] = { duration: 5 };
      }
    }
  }

  return result;
}

/** Returns the first pending step in order 1..5, or 5 if all are completed. */
export function initialStepFromCompleted(completed: CompletedSteps): number {
  for (let s = 1; s <= GUIDED_STEP_COUNT; s++) {
    if (!completed[s]) return s;
  }
  return GUIDED_STEP_COUNT;
}

export function areCompletedStepsEqual(a: CompletedSteps, b: CompletedSteps): boolean {
  const keysA = Object.keys(a);
  const keysB = Object.keys(b);
  if (keysA.length !== keysB.length) return false;
  for (const k of keysA) {
    const numK = Number(k);
    if (!b[numK] || b[numK].sampleId !== a[numK].sampleId || b[numK].duration !== a[numK].duration) {
      return false;
    }
  }
  return true;
}

