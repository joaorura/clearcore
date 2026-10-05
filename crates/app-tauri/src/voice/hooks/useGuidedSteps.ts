import { useEffect, useRef, useState } from 'react';
import { GUIDED_STEP_COUNT, STEP_QUESTIONS, replacedSampleToDelete, stepAfterSubmit, type CompletedSteps } from './guidedSteps';
import type { EnrollmentRecorder } from './useEnrollmentRecorder';
import type { Translate } from './voiceProfileLogic';

/** UI preference only (open questions vs reading); no sample, take or profile data is stored locally. */
const STORAGE_READING_MODE_KEY = 'clearcore_voice_reading_mode';

export interface GuidedSteps {
  currentStep: number;
  setCurrentStep: (step: number) => void;
  isReadingMode: boolean;
  completedSteps: CompletedSteps;
  startStep: (step: number) => Promise<void>;
  finishStep: (step: number) => Promise<void>;
  redoStep: (step: number) => void;
  nextStep: () => void;
  toggleReadingMode: () => void;
  resetSteps: () => void;
}

/** The 5-step guided enrollment: each accepted take is one sample in the service. */
export function useGuidedSteps(opts: {
  recorder: EnrollmentRecorder;
  t: Translate;
  flash: (message: string, ms: number) => void;
  /** Deletes a sample in the service (the old take of a step recorded again). */
  deleteReplacedSample: (sampleId: string) => Promise<unknown>;
}): GuidedSteps {
  const { recorder, t, flash, deleteReplacedSample } = opts;
  const [currentStep, setCurrentStep] = useState<number>(1);
  const [isReadingMode, setIsReadingMode] = useState<boolean>(false);
  const [completedSteps, setCompletedSteps] = useState<CompletedSteps>({});
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

  const finishStep = async (step: number) => {
    const captured = await recorder.stopCapture();
    if (!captured) return;

    const previous = completedRef.current[step];
    const { outcome, sampleId } = await recorder.submitSample(captured, t(STEP_QUESTIONS[step - 1].categoryKey), 'enroll');
    // A failed take keeps the step (and its sample in the service) as it was.
    if (outcome.kind !== 'done') return;
    setCompletedSteps((prev) => {
      const take = stepAfterSubmit(prev[step], outcome, sampleId, captured);
      return take ? { ...prev, [step]: take } : prev;
    });
    flash(t('voiceProfile.sampleCompleted'), 3500);
    if (step < GUIDED_STEP_COUNT) {
      setCurrentStep(step + 1);
    }
    // Known limit: the new sample is added BEFORE the old one is deleted, so the old one still
    // counts against the 90 s budget while the new one is checked. With the budget almost full,
    // the new take is refused with ENROLL_BUDGET_EXCEEDED: safe (nothing is lost, the old sample
    // stays) and the card opens the gallery so the user can free speech first.
    const replaced = replacedSampleToDelete(previous, outcome, sampleId);
    if (replaced !== null) await deleteReplacedSample(replaced);
  };

  // Up to MAX_RECORD_SECONDS with manual stop; the limit finishes the step by itself.
  const startStep = async (step: number) => {
    await recorder.startCapture(() => {
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
    currentStep, setCurrentStep, isReadingMode, completedSteps,
    startStep, finishStep, redoStep,
    nextStep: () => { if (currentStep < GUIDED_STEP_COUNT) setCurrentStep(currentStep + 1); },
    toggleReadingMode,
    // "Full re-enrollment" only restarts the guided flow here: it clears completedSteps and so
    // forgets the steps' sampleIds. The old samples stay in the service and keep using the speech
    // budget; nothing deletes them automatically (spec §4.4) — the user deletes them in the gallery.
    resetSteps: () => { setCompletedSteps({}); setCurrentStep(1); },
  };
}
