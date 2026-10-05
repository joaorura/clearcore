import { useState } from 'react';
import type { EnrollmentJob, EnrollmentLabels } from '../enrollmentTypes';
import { errorLabel } from '../enrollmentErrors';
import { errorLabelForJob, shouldOpenGalleryOnError, type JobOutcome, type Translate } from './voiceProfileLogic';

/** Where a job was started; its feedback is shown there (same ids as the card tabs). */
export type JobOrigin = 'enroll' | 'gallery' | 'calls' | 'profile';
/** What the job does: store a sample, build the profile, or approve a call take. */
export type JobKind = 'sample' | 'build' | 'take';
/** The running job, or the last one whose result is still shown. */
export interface JobSource { origin: JobOrigin; kind: JobKind }

export interface JobFeedback {
  currentJob: EnrollmentJob | null;
  jobBusy: boolean;
  enrollErrorText: string | null;
  budgetError: { remainingSeconds: number | null } | null;
  source: JobSource | null;
  setCurrentJob: (job: EnrollmentJob | null) => void;
  setJobBusy: (busy: boolean) => void;
  setEnrollErrorText: (text: string | null) => void;
  setBudgetError: (b: { remainingSeconds: number | null } | null) => void;
  /** Clears the previous feedback and marks a job of `kind` as running, started on `origin`. */
  begin: (origin: JobOrigin, kind?: JobKind) => void;
  /** Clears error and job (e.g. when the voluntary sample modal opens or closes). */
  clearMessages: () => void;
  /** Applies a non-done outcome; the budget error calls `onBudgetError` (the card opens the gallery). */
  /** `kind` picks the error label by origin (a build's ENROLL_TOO_LITTLE_SPEECH differs). */
  applyOutcome: (outcome: JobOutcome, kind?: JobKind) => void;
  /** Shows the error of a request that threw (timeout or unreachable service). */
  failWith: (err: unknown) => void;
  /** Error of a call take approval (`kind: 'take'`). */
  showError: (origin: JobOrigin, text: string) => void;
  showBudgetError: (remainingSeconds: number | null) => void;
}

/** Shared job state: stage while running, quality when done, error/budget prompt when not. */
export function useJobFeedback(t: Translate, labels: EnrollmentLabels, onBudgetError: () => void): JobFeedback {
  const [currentJob, setCurrentJob] = useState<EnrollmentJob | null>(null);
  const [jobBusy, setJobBusy] = useState<boolean>(false);
  const [enrollErrorText, setEnrollErrorText] = useState<string | null>(null);
  const [budgetError, setBudgetError] = useState<{ remainingSeconds: number | null } | null>(null);
  const [source, setSource] = useState<JobSource | null>(null);

  const showBudgetError = (remainingSeconds: number | null) => {
    setBudgetError({ remainingSeconds });
    onBudgetError();
  };

  return {
    currentJob, jobBusy, enrollErrorText, budgetError, source,
    setCurrentJob, setJobBusy, setEnrollErrorText, setBudgetError,
    begin: (o, kind = 'sample') => {
      setBudgetError(null);
      setEnrollErrorText(null);
      setCurrentJob(null);
      setSource({ origin: o, kind });
      setJobBusy(true);
    },
    clearMessages: () => {
      setEnrollErrorText(null);
      setCurrentJob(null);
    },
    applyOutcome: (outcome, kind = 'sample') => {
      if (outcome.kind === 'done') return;
      setCurrentJob(null);
      if (outcome.kind === 'show-budget-error') {
        showBudgetError(outcome.remainingSeconds);
        return;
      }
      if (shouldOpenGalleryOnError(outcome.code)) onBudgetError();
      setEnrollErrorText(errorLabelForJob(outcome.code, kind, labels, t));
    },
    failWith: (err) => {
      setCurrentJob(null);
      setEnrollErrorText(
        err instanceof Error && err.message === 'timeout' ? t('voiceProfile.jobTimeout') : errorLabel('SERVICE_UNAVAILABLE', labels),
      );
    },
    showError: (o, text) => {
      setSource({ origin: o, kind: 'take' });
      setEnrollErrorText(text);
    },
    showBudgetError,
  };
}
