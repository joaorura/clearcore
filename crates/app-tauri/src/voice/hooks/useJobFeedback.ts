import { useState } from 'react';
import type { EnrollmentJob, EnrollmentLabels } from '../enrollmentTypes';
import { errorLabel } from '../enrollmentErrors';
import { shouldOpenGalleryOnError, type JobOutcome, type Translate } from './voiceProfileLogic';

/** Where a job was started; its feedback is shown there (same ids as the card tabs). */
export type JobOrigin = 'enroll' | 'gallery' | 'calls' | 'profile';

export interface JobFeedback {
  currentJob: EnrollmentJob | null;
  jobBusy: boolean;
  enrollErrorText: string | null;
  budgetError: { remainingSeconds: number | null } | null;
  origin: JobOrigin | null;
  setCurrentJob: (job: EnrollmentJob | null) => void;
  setJobBusy: (busy: boolean) => void;
  setEnrollErrorText: (text: string | null) => void;
  setBudgetError: (b: { remainingSeconds: number | null } | null) => void;
  /** Clears the previous feedback and marks a job as running for `origin`. */
  begin: (origin: JobOrigin) => void;
  /** Clears error and job (e.g. when the voluntary sample modal opens or closes). */
  clearMessages: () => void;
  /** Applies a non-done outcome; the budget error calls `onBudgetError` (the card opens the gallery). */
  applyOutcome: (outcome: JobOutcome) => void;
  /** Shows the error of a request that threw (timeout or unreachable service). */
  failWith: (err: unknown) => void;
  showError: (origin: JobOrigin, text: string) => void;
  showBudgetError: (remainingSeconds: number | null) => void;
}

/** Shared job state: stage while running, quality when done, error/budget prompt when not. */
export function useJobFeedback(t: Translate, labels: EnrollmentLabels, onBudgetError: () => void): JobFeedback {
  const [currentJob, setCurrentJob] = useState<EnrollmentJob | null>(null);
  const [jobBusy, setJobBusy] = useState<boolean>(false);
  const [enrollErrorText, setEnrollErrorText] = useState<string | null>(null);
  const [budgetError, setBudgetError] = useState<{ remainingSeconds: number | null } | null>(null);
  const [origin, setOrigin] = useState<JobOrigin | null>(null);

  const showBudgetError = (remainingSeconds: number | null) => {
    setBudgetError({ remainingSeconds });
    onBudgetError();
  };

  return {
    currentJob, jobBusy, enrollErrorText, budgetError, origin,
    setCurrentJob, setJobBusy, setEnrollErrorText, setBudgetError,
    begin: (o) => {
      setBudgetError(null);
      setEnrollErrorText(null);
      setCurrentJob(null);
      setOrigin(o);
      setJobBusy(true);
    },
    clearMessages: () => {
      setEnrollErrorText(null);
      setCurrentJob(null);
    },
    applyOutcome: (outcome) => {
      if (outcome.kind === 'done') return;
      setCurrentJob(null);
      if (outcome.kind === 'show-budget-error') {
        showBudgetError(outcome.remainingSeconds);
        return;
      }
      if (shouldOpenGalleryOnError(outcome.code)) onBudgetError();
      setEnrollErrorText(errorLabel(outcome.code, labels));
    },
    failWith: (err) => {
      setCurrentJob(null);
      setEnrollErrorText(
        err instanceof Error && err.message === 'timeout' ? t('voiceProfile.jobTimeout') : errorLabel('SERVICE_UNAVAILABLE', labels),
      );
    },
    showError: (o, text) => {
      setOrigin(o);
      setEnrollErrorText(text);
    },
    showBudgetError,
  };
}
