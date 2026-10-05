import { describe, it, expect } from 'vitest';
import { JobAbortScope, isAbortError } from '../jobAbort';
import { waitForJob } from '../enrollmentClient';
import type { EnrollmentJob } from '../enrollmentTypes';

const running: EnrollmentJob = { jobId: 'sample-job-1', state: 'running', stage: 'trim', errorCode: null, remainingSeconds: null, sampleId: null, profileId: null, quality: null };

describe('JobAbortScope', () => {
  it('hands out live signals and aborts all of them on unmount', () => {
    const scope = new JobAbortScope();
    const a = scope.signal();
    const b = scope.signal();
    expect(a.aborted || b.aborted).toBe(false);
    scope.abortAll();
    expect(a.aborted && b.aborted).toBe(true);
    // after unmount every new signal is born aborted: no poller may start
    expect(scope.signal().aborted).toBe(true);
  });
  it('releases a finished job signal', () => {
    const scope = new JobAbortScope();
    const s = scope.signal();
    scope.release(s);
    scope.abortAll();
    expect(s.aborted).toBe(false);
  });
  it('stops a running waitForJob when the card unmounts', async () => {
    const scope = new JobAbortScope();
    const p = waitForJob('sample-job-1', { intervalMs: 5, getJob: async () => running, signal: scope.signal() });
    scope.abortAll();
    await expect(p).rejects.toThrow('aborted');
    let caught: unknown;
    try { await p; } catch (e) { caught = e; }
    expect(isAbortError(caught)).toBe(true);
    expect(isAbortError(new Error('timeout'))).toBe(false);
  });
});
