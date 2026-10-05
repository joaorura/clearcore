import { describe, it, expect } from 'vitest';
import { ptBR } from '../i18n/locales/pt-BR';
import { enUS } from '../i18n/locales/en-US';
import type { EnrollErrorCode, EnrollmentJob, EnrollmentLabels } from '../voice/enrollmentTypes';
import {
  buildEnrollmentLabels,
  nextStepAfterJob,
  shouldOpenGalleryOnError,
  shouldShowTakeError,
  formatSecondsForLocale,
  interpolateBudgetBody,
} from '../VoiceProfileCard';

const getNested = (obj: unknown, path: string): string | undefined => {
  let cur: unknown = obj;
  for (const part of path.split('.')) {
    if (cur && typeof cur === 'object' && part in (cur as Record<string, unknown>)) {
      cur = (cur as Record<string, unknown>)[part];
    } else {
      return undefined;
    }
  }
  return typeof cur === 'string' ? cur : undefined;
};

/** A translator that marks missing keys instead of silently returning the path. */
const strictT = (translations: unknown) => (path: string): string => getNested(translations, path) ?? `MISSING:${path}`;

const ERROR_CODES: Array<EnrollErrorCode | 'UNKNOWN'> = [
  'ENROLL_CLIPPING', 'ENROLL_TOO_QUIET', 'ENROLL_TOO_LITTLE_SPEECH', 'ENROLL_MODEL_NOT_CONFIGURED',
  'ENROLL_BUDGET_EXCEEDED', 'ENROLL_INVALID_AUDIO', 'ENROLL_PAYLOAD_TOO_LARGE',
  'ENROLL_JOB_NOT_FOUND', 'ENROLL_BUSY', 'ENROLL_FAILED', 'SERVICE_UNAVAILABLE', 'UNKNOWN',
];

const LABEL_KEYS: Array<Exclude<keyof EnrollmentLabels, 'errors'>> = [
  'budgetTitle', 'budgetUsed', 'budgetRemaining', 'seconds', 'budgetExceededTitle', 'budgetExceededBody',
  'deleteAction', 'deleting', 'otherMicrophone', 'needsReenroll', 'usedInProfile', 'notUsed',
  'stageQueued', 'stageDenoise', 'stageTrim', 'stageEq', 'stageEnroll', 'stageApply',
  'jobDone', 'jobFailed', 'devModelNotice', 'qualityPeak', 'qualityLevel', 'qualitySpeech',
];

const job = (over: Partial<EnrollmentJob>): EnrollmentJob => ({
  jobId: 'job-1', state: 'done', stage: 'apply', errorCode: null, remainingSeconds: null,
  sampleId: null, profileId: null, quality: null, ...over,
});

describe('buildEnrollmentLabels', () => {
  for (const [name, translations] of [['pt-BR', ptBR], ['en-US', enUS]] as const) {
    it(`defines every EnrollmentLabels key in ${name}`, () => {
      const labels = buildEnrollmentLabels(strictT(translations));
      for (const key of LABEL_KEYS) {
        expect(labels[key], key).toBeTypeOf('string');
        expect(labels[key], key).not.toMatch(/^MISSING:/);
        expect(labels[key].length, key).toBeGreaterThan(0);
      }
      for (const code of ERROR_CODES) {
        expect(labels.errors[code], code).toBeTypeOf('string');
        expect(labels.errors[code], code).not.toMatch(/^MISSING:/);
      }
      expect(labels.budgetExceededBody).toContain('{remaining}');
    });
  }

  it('uses the agreed Brazilian Portuguese texts', () => {
    const labels = buildEnrollmentLabels(strictT(ptBR));
    expect(labels.devModelNotice).toBe('Modelo de enrollment de desenvolvimento, ainda não aprovado');
    expect(labels.budgetExceededTitle).toBe('Limite de 90 s de fala atingido');
    expect(labels.budgetExceededBody).toBe('Restam {remaining} s. Apague algum áudio da galeria para adicionar este.');
    expect(labels.otherMicrophone).toBe('Outro microfone (não usado)');
    expect(labels.needsReenroll).toBe('Regravar');
    expect(labels.errors.ENROLL_BUSY).toBe('O serviço está ocupado processando outro áudio. Aguarde alguns segundos e tente de novo.');
  });

  it('gives ENROLL_BUSY its own label, distinct from the generic failure', () => {
    for (const translations of [ptBR, enUS]) {
      const labels = buildEnrollmentLabels(strictT(translations));
      expect(labels.errors.ENROLL_BUSY).not.toBe(labels.errors.ENROLL_FAILED);
    }
    expect(buildEnrollmentLabels(strictT(enUS)).errors.ENROLL_BUSY).toBe('The service is busy with another recording. Wait a few seconds and try again.');
  });
});

describe('nextStepAfterJob', () => {
  it('is done for a finished job and carries its quality', () => {
    const q = { peak: 0.5, rmsDbfs: -20, activeFraction: 0.8, speechSeconds: 4.2 };
    expect(nextStepAfterJob(job({ state: 'done', quality: q }))).toEqual({ kind: 'done', quality: q });
  });
  it('shows the budget error with the remaining seconds', () => {
    expect(nextStepAfterJob(job({ state: 'failed', errorCode: 'ENROLL_BUDGET_EXCEEDED', remainingSeconds: 2.5 })))
      .toEqual({ kind: 'show-budget-error', remainingSeconds: 2.5 });
  });
  it('shows a plain error for any other failure, defaulting to ENROLL_FAILED', () => {
    expect(nextStepAfterJob(job({ state: 'failed', errorCode: 'ENROLL_TOO_QUIET' }))).toEqual({ kind: 'show-error', code: 'ENROLL_TOO_QUIET' });
    expect(nextStepAfterJob(job({ state: 'failed', errorCode: null }))).toEqual({ kind: 'show-error', code: 'ENROLL_FAILED' });
  });
});

describe('shouldOpenGalleryOnError', () => {
  it('opens the gallery only for the budget error', () => {
    expect(shouldOpenGalleryOnError('ENROLL_BUDGET_EXCEEDED')).toBe(true);
    for (const code of ERROR_CODES.filter((c) => c !== 'ENROLL_BUDGET_EXCEEDED' && c !== 'UNKNOWN') as EnrollErrorCode[]) {
      expect(shouldOpenGalleryOnError(code), code).toBe(false);
    }
    expect(shouldOpenGalleryOnError(null)).toBe(false);
  });
});

describe('shouldShowTakeError', () => {
  it('hides the error of a dynamic take not recorded for lack of margin', () => {
    expect(shouldShowTakeError({ recorded: false, reason: 'budget' })).toBe(false);
  });
  it('shows the error when the user approves a take that no longer fits', () => {
    expect(shouldShowTakeError({ errorCode: 'ENROLL_BUDGET_EXCEEDED' })).toBe(true);
    expect(shouldShowTakeError({ errorCode: 'SERVICE_UNAVAILABLE' })).toBe(true);
  });
  it('shows nothing on success', () => {
    expect(shouldShowTakeError({ success: true })).toBe(false);
    expect(shouldShowTakeError(undefined)).toBe(false);
  });
});

describe('seconds presentation', () => {
  it('uses a decimal comma in pt-BR and a point in en-US', () => {
    expect(formatSecondsForLocale(84.25, 'pt-BR')).toBe('84,3');
    expect(formatSecondsForLocale(84.25, 'en-US')).toBe('84.3');
  });
  it('interpolates the remaining seconds into the budget body, localized', () => {
    expect(interpolateBudgetBody('Restam {remaining} s.', 2.5, 'pt-BR')).toBe('Restam 2,5 s.');
    expect(interpolateBudgetBody('Restam {remaining} s.', null, 'pt-BR')).toBe('Restam {remaining} s.');
  });
});
