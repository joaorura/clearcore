import { useCallback, useEffect, useRef, useState } from 'react';
import type { ServiceSample } from '../enrollmentTypes';
import {
  GUIDED_STEP_COUNT,
  STEP_QUESTIONS,
  initialStepFromCompleted,
  replacedSampleToDelete,
  stepAfterSubmit,
  syncCompletedStepsFromSamples,
  type CompletedSteps,
} from './guidedSteps';
import type { EnrollmentRecorder } from './useEnrollmentRecorder';
import type { Translate } from './voiceProfileLogic';
import { logVoiceDebug } from '../enrollmentClient';

/** UI preference only (open questions vs reading); no sample, take or profile data is stored locally. */
const STORAGE_READING_MODE_KEY = 'clearcore_voice_reading_mode';

export interface GuidedSteps {
  currentStep: number;
  setCurrentStep: (step: number) => void;
  isReadingMode: boolean;
  completedSteps: CompletedSteps;
  setCompletedSteps: React.Dispatch<React.SetStateAction<CompletedSteps>>;
  startStep: (step: number) => Promise<void>;
  finishStep: (step: number) => Promise<void>;
  redoStep: (step: number) => void;
  nextStep: () => void;
  toggleReadingMode: () => void;
  resetSteps: () => void;
  isSubmitting: boolean;
  syncFromSamples: (samples: ServiceSample[], advanceStep?: boolean, isEnrolled?: boolean) => void;
}

/** The 5-step guided enrollment: each accepted take is one sample in the service. */
export function useGuidedSteps(opts: {
  recorder: EnrollmentRecorder;
  t: Translate;
  flash: (message: string, ms: number) => void;
  /** Deletes a sample in the service (the old take of a step recorded again). */
  deleteReplacedSample: (sampleId: string) => Promise<unknown>;
  samples?: ServiceSample[];
}): GuidedSteps {
  const { recorder, t, flash, deleteReplacedSample, samples } = opts;
  const [currentStep, setCurrentStep] = useState<number>(1);
  const [isReadingMode, setIsReadingMode] = useState<boolean>(false);
  const [isSubmitting, setIsSubmitting] = useState<boolean>(false);
  const [completedSteps, setCompletedSteps] = useState<CompletedSteps>({});
  const hasExplicitlyResetRef = useRef<boolean>(false);
  const hasAutoInitializedStepRef = useRef<boolean>(false);

  // Read at the end of a recording (the limit timer holds an older closure).
  const completedRef = useRef<CompletedSteps>(completedSteps);
  completedRef.current = completedSteps;

  useEffect(() => {
    try {
      setIsReadingMode(localStorage.getItem(STORAGE_READING_MODE_KEY) === 'true');
    } catch {
      // UI preference only
    }
  }, []);

  const syncFromSamples = useCallback((newSamples: ServiceSample[], advanceStep = false, isEnrolled = false) => {
    if (hasExplicitlyResetRef.current) return;
    setCompletedSteps((prev) => {
      const synced = syncCompletedStepsFromSamples(newSamples, prev, t, isEnrolled);
      if (advanceStep && Object.keys(synced).length > 0) {
        setCurrentStep(initialStepFromCompleted(synced));
      }
      return synced;
    });
  }, [t]);

  // Synchronize wizard steps when existing daemon gallery samples load or update
  useEffect(() => {
    if (!samples || hasExplicitlyResetRef.current) return;
    setCompletedSteps((prev) => {
      const synced = syncCompletedStepsFromSamples(samples, prev, t);
      if (!hasAutoInitializedStepRef.current && samples.length > 0) {
        hasAutoInitializedStepRef.current = true;
        setCurrentStep(initialStepFromCompleted(synced));
      }
      return synced;
    });
  }, [samples, t]);

  const finishStep = async (step: number) => {
    logVoiceDebug('GUIDED_STEPS', `finishStep(${step}) called, isSubmitting=true`);
    setIsSubmitting(true);
    try {
      const captured = await recorder.stopCapture();
      if (!captured) {
        logVoiceDebug('GUIDED_STEPS', `finishStep(${step}): recorder.stopCapture returned null`);
        return;
      }

      const previous = completedRef.current[step];
      const category = t(STEP_QUESTIONS[step - 1].categoryKey);
      logVoiceDebug('GUIDED_STEPS', `Submitting sample for step ${step}, category=${category}...`);
      const { outcome, sampleId } = await recorder.submitSample(captured, category, 'enroll');
      logVoiceDebug('GUIDED_STEPS', `submitSample result for step ${step}:`, { outcome, sampleId });
      // A failed take keeps the step (and its sample in the service) as it was.
      if (outcome.kind !== 'done') {
        logVoiceDebug('GUIDED_STEPS', `finishStep(${step}) NOT done:`, outcome);
        console.warn('[VoiceProfile] finishStep not done:', outcome);
        return;
      }
      hasExplicitlyResetRef.current = false;
      setCompletedSteps((prev) => {
        const take = stepAfterSubmit(prev[step], outcome, sampleId, captured);
        logVoiceDebug('GUIDED_STEPS', `Step ${step} updated in completedSteps:`, take);
        return take ? { ...prev, [step]: take } : prev;
      });
      flash(t('voiceProfile.sampleCompleted'), 4000);
      logVoiceDebug('GUIDED_STEPS', `Step ${step} finished successfully! Remaining on step ${step} with [Refazer] and [Próximo Passo] buttons.`);

      const replaced = replacedSampleToDelete(previous, outcome, sampleId);
      if (replaced !== null) await deleteReplacedSample(replaced);
    } finally {
      setIsSubmitting(false);
      logVoiceDebug('GUIDED_STEPS', `finishStep(${step}) finally block: isSubmitting=false`);
    }
  };

  // Up to MAX_RECORD_SECONDS with manual stop; the limit finishes the step by itself.
  const startStep = async (step: number) => {
    logVoiceDebug('GUIDED_STEPS', `startStep(${step}) called, starting recorder...`);
    await recorder.startCapture(() => {
      logVoiceDebug('GUIDED_STEPS', `Time limit reached for step ${step}, calling finishStep...`);
      void finishStep(step);
    });
  };

  // The step keeps its current take until the new one is accepted (then the old sample is deleted).
  const redoStep = (step: number) => {
    void startStep(step);
  };

  const toggleReadingMode = () => {
    const nextVal = !isReadingMode;
    setIsReadingMode(nextVal);
    try {
      localStorage.setItem(STORAGE_READING_MODE_KEY, String(nextVal));
    } catch {
      // UI preference only
    }
  };

  return {
    currentStep, setCurrentStep, isReadingMode, completedSteps, setCompletedSteps,
    startStep, finishStep, redoStep,
    nextStep: () => { if (currentStep < GUIDED_STEP_COUNT) setCurrentStep(currentStep + 1); },
    toggleReadingMode,
    isSubmitting,
    syncFromSamples,
    // "Full re-enrollment" only restarts the guided flow here: it clears completedSteps and so
    // forgets the steps' sampleIds. The old samples stay in the service and keep using the speech
    // budget; nothing deletes them automatically (spec §4.4) — the user deletes them in the gallery.
    resetSteps: () => {
      hasExplicitlyResetRef.current = true;
      setCompletedSteps({});
      setCurrentStep(1);
    },
  };
}
