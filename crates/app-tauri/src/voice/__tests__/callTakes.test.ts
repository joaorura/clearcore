import { describe, it, expect } from 'vitest';
import { classifyTakeApproval } from '../hooks/voiceProfileLogic';

describe('classifyTakeApproval', () => {
  it('is approved on success and when a dynamic take was skipped for lack of margin', () => {
    expect(classifyTakeApproval({ success: true })).toEqual({ kind: 'approved' });
    expect(classifyTakeApproval({ recorded: false, reason: 'budget' })).toEqual({ kind: 'approved' });
  });
  it('is a budget error with the remaining seconds when the take no longer fits', () => {
    expect(classifyTakeApproval({ errorCode: 'ENROLL_BUDGET_EXCEEDED', remainingSeconds: 3.5 })).toEqual({ kind: 'budget', remainingSeconds: 3.5 });
    expect(classifyTakeApproval({ errorCode: 'ENROLL_BUDGET_EXCEEDED', remainingSeconds: 'x' })).toEqual({ kind: 'budget', remainingSeconds: null });
  });
  it('is a plain error for any other code, unknown text collapsing to ENROLL_FAILED', () => {
    expect(classifyTakeApproval({ errorCode: 'SERVICE_UNAVAILABLE' })).toEqual({ kind: 'error', code: 'SERVICE_UNAVAILABLE' });
    expect(classifyTakeApproval({ errorCode: 'free text from the service' })).toEqual({ kind: 'error', code: 'ENROLL_FAILED' });
  });
});
