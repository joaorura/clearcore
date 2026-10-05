import { useEffect, useState } from 'react';
import { GUIDED_STEP_COUNT, STEP_QUESTIONS, stepAfterSubmit, type CompletedSteps } from './guidedSteps';
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
}): GuidedSteps {
  const { recorder, t, flash } = opts;
  const [currentStep, setCurrentStep] = useState<number>(1);
  const [isReadingMode, setIsReadingMode] = useState<boolean>(false);
  const [completedSteps, setCompletedSteps] = useState<CompletedSteps>({});

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

    const { outcome, sampleId } = await recorder.submitSample(captured, t(STEP_QUESTIONS[step - 1].categoryKey), 'enroll');
    if (outcome.kind !== 'done') return;
    setCompletedSteps((prev) => {
      const take = stepAfterSubmit(prev[step], outcome, sampleId, captured);
      return take ? { ...prev, [step]: take } : prev;
    });
    flash(t('voiceProfile.sampleCompleted'), 3500);
  };

  // Up to MAX_RECORD_SECONDS with manual stop; the limit finishes the step by itself.
  const startStep = async (step: number) => {
    await recorder.startCapture(() => {
      void finishStep(step);
    });
  };

  const redoStep = (step: number) => {
    setCompletedSteps((prev) => {
      const copy = { ...prev };
      delete copy[step];
      return copy;
    });
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
    resetSteps: () => { setCompletedSteps({}); setCurrentStep(1); },
  };
}
