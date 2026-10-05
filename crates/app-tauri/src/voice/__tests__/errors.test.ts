import { it, expect } from 'vitest';
import { enrollmentErrorCode, isBudgetError, errorLabel } from '../enrollmentErrors';
import type { EnrollmentLabels, EnrollErrorCode } from '../enrollmentTypes';

const codes: Array<EnrollErrorCode | 'UNKNOWN'> = ['ENROLL_CLIPPING','ENROLL_TOO_QUIET','ENROLL_TOO_LITTLE_SPEECH','ENROLL_MODEL_NOT_CONFIGURED','ENROLL_BUDGET_EXCEEDED','ENROLL_INVALID_AUDIO','ENROLL_PAYLOAD_TOO_LARGE','ENROLL_JOB_NOT_FOUND','ENROLL_FAILED','SERVICE_UNAVAILABLE','UNKNOWN'];
const labels = { errors: Object.fromEntries(codes.map((c) => [c, `label:${c}`])) } as unknown as EnrollmentLabels;

it('unknown service codes never leak as text', () => {
  expect(enrollmentErrorCode({ errorCode: 'ENROLL_BUDGET_EXCEEDED' })).toBe('ENROLL_BUDGET_EXCEEDED');
  expect(enrollmentErrorCode({ errorCode: 'something <script>' })).toBe('ENROLL_FAILED');
  expect(enrollmentErrorCode({ ok: true })).toBeNull();
});
it('reads the errorCode of a failed job and ignores null/non-objects', () => {
  expect(enrollmentErrorCode({ state: 'failed', errorCode: 'ENROLL_TOO_QUIET' })).toBe('ENROLL_TOO_QUIET');
  expect(enrollmentErrorCode({ state: 'done', errorCode: null })).toBeNull();
  expect(enrollmentErrorCode(null)).toBeNull();
  expect(enrollmentErrorCode('ENROLL_CLIPPING')).toBeNull();
});
it('isBudgetError only for the budget code', () => {
  expect(isBudgetError('ENROLL_BUDGET_EXCEEDED')).toBe(true);
  expect(isBudgetError('ENROLL_FAILED')).toBe(false);
  expect(isBudgetError(null)).toBe(false);
});
it('errorLabel returns only label strings', () => {
  expect(errorLabel('ENROLL_CLIPPING', labels)).toBe('label:ENROLL_CLIPPING');
  expect(errorLabel(null, labels)).toBe('label:UNKNOWN');
});
