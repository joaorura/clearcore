import type { EnrollErrorCode, EnrollmentLabels } from './enrollmentTypes';

const KNOWN: ReadonlySet<string> = new Set<EnrollErrorCode>([
  'ENROLL_CLIPPING', 'ENROLL_TOO_QUIET', 'ENROLL_TOO_LITTLE_SPEECH', 'ENROLL_MODEL_NOT_CONFIGURED',
  'ENROLL_BUDGET_EXCEEDED', 'ENROLL_INVALID_AUDIO', 'ENROLL_PAYLOAD_TOO_LARGE',
  'ENROLL_JOB_NOT_FOUND', 'ENROLL_FAILED', 'SERVICE_UNAVAILABLE',
]);

/** Reads `{ errorCode }` (or a failed job's errorCode). Unknown codes/text collapse to ENROLL_FAILED. */
export function enrollmentErrorCode(res: unknown): EnrollErrorCode | null {
  if (typeof res !== 'object' || res === null || !('errorCode' in res)) return null;
  const code = (res as { errorCode: unknown }).errorCode;
  if (code === null || code === undefined) return null;
  return typeof code === 'string' && KNOWN.has(code) ? (code as EnrollErrorCode) : 'ENROLL_FAILED';
}

export function isBudgetError(code: EnrollErrorCode | null): boolean {
  return code === 'ENROLL_BUDGET_EXCEEDED';
}

/** Only ever returns strings from `labels`; never free text from the service. */
export function errorLabel(code: EnrollErrorCode | null, labels: EnrollmentLabels): string {
  return (code && labels.errors[code]) || labels.errors.UNKNOWN;
}
