import { describe, it, expect, vi } from 'vitest';
import { JobAbortScope, ensureOpenScope, isAbortError } from '../jobAbort';
import { waitForJob } from '../enrollmentClient';
import { normalizeActiveJobs, resolveBuildJob, type ActiveEnrollmentJob, type BuildFlowDeps } from '../profileBuildFlow';
import type { EnrollmentJob } from '../enrollmentTypes';

const job = (state: EnrollmentJob['state'], stage: EnrollmentJob['stage'] = 'enroll'): EnrollmentJob => ({
  jobId: 'profile-job-1', state, stage, errorCode: null, remainingSeconds: null, sampleId: null, profileId: state === 'done' ? 'p' : null, quality: null,
});

describe('profile build survives React StrictMode (mount, unmount, mount)', () => {
  it('the scope closed by the simulated unmount would kill the poller; ensureOpenScope gives a live one', async () => {
    let scope = new JobAbortScope();
    scope.abortAll(); // StrictMode cleanup on a card that is still mounted
    const dead = waitForJob('profile-job-1', { intervalMs: 1, getJob: async () => job('running'), signal: scope.signal() });
    await expect(dead).rejects.toSatisfy(isAbortError);

    scope = ensureOpenScope(scope);
    expect(scope.isClosed()).toBe(false);
    const getJob = vi.fn()
      .mockResolvedValueOnce(job('running', 'denoise'))
      .mockResolvedValueOnce(job('running', 'enroll'))
      .mockResolvedValue(job('done', 'apply'));
    const updates: string[] = [];
    const signal = scope.signal();
    const result = await waitForJob('profile-job-1', { intervalMs: 1, getJob, signal, onUpdate: (j) => updates.push(j.stage) });
    scope.release(signal);
    expect(result.state).toBe('done');
    expect(getJob).toHaveBeenCalledTimes(3);
    expect(updates).toEqual(['denoise', 'enroll', 'apply']);
  });
  it('keeps an open scope as is and a real unmount still aborts its pollers', async () => {
    const open = new JobAbortScope();
    expect(ensureOpenScope(open)).toBe(open);
    expect(ensureOpenScope(null)).toBeInstanceOf(JobAbortScope);
    const p = waitForJob('profile-job-1', { intervalMs: 5, getJob: async () => job('running'), signal: open.signal() });
    open.abortAll();
    await expect(p).rejects.toSatisfy(isAbortError);
  });
});

describe('normalizeActiveJobs', () => {
  it('reads the daemon wire format', () => {
    expect(normalizeActiveJobs([{ job_id: 'profile-job-3', kind: 'profile', stage: 'enroll' }, { job_id: 'sample-job-9', kind: 'sample', stage: 'trim' }]))
      .toEqual([{ jobId: 'profile-job-3', kind: 'profile', stage: 'enroll' }, { jobId: 'sample-job-9', kind: 'sample', stage: 'trim' }]);
  });
  it('treats absence and garbage as no jobs', () => {
    expect(normalizeActiveJobs(undefined)).toEqual([]);
    expect(normalizeActiveJobs(null)).toEqual([]);
    expect(normalizeActiveJobs('x')).toEqual([]);
    expect(normalizeActiveJobs([null, 3, { kind: 'profile' }, { job_id: 'a', kind: 'other' }, { job_id: '', kind: 'sample' }])).toEqual([]);
  });
});

describe('resolveBuildJob', () => {
  const busy = { errorCode: 'ENROLL_BUSY' as const };
  const make = (over: Partial<BuildFlowDeps>): BuildFlowDeps => ({
    startBuild: async () => ({ jobId: 'profile-job-1' }),
    activeJobs: async () => [],
    waitForSampleJob: async () => job('done'),
    sleep: async () => undefined,
    ...over,
  });
  it('returns the new job when the daemon accepts the build', async () => {
    expect(await resolveBuildJob(make({}))).toEqual({ jobId: 'profile-job-1' });
  });
  it('passes other errors through (never swallowed)', async () => {
    const r = await resolveBuildJob(make({ startBuild: async () => ({ errorCode: 'ENROLL_TOO_LITTLE_SPEECH' as const }) }));
    expect(r).toEqual({ errorCode: 'ENROLL_TOO_LITTLE_SPEECH' });
  });
  it('on ENROLL_BUSY follows the profile job that is already running', async () => {
    const activeJobs = async (): Promise<ActiveEnrollmentJob[]> => [{ jobId: 'profile-job-7', kind: 'profile', stage: 'enroll' }];
    expect(await resolveBuildJob(make({ startBuild: async () => busy, activeJobs }))).toEqual({ jobId: 'profile-job-7' });
  });
  it('on ENROLL_BUSY with only a sample running, waits for it and asks again', async () => {
    const startBuild = vi.fn().mockResolvedValueOnce(busy).mockResolvedValueOnce({ jobId: 'profile-job-2' });
    const waiting: boolean[] = [];
    const waitForSampleJob = vi.fn(async () => job('done'));
    const r = await resolveBuildJob(make({
      startBuild,
      activeJobs: async () => [{ jobId: 'sample-job-4', kind: 'sample', stage: 'trim' }],
      waitForSampleJob,
      onProcessingSample: (w) => waiting.push(w),
    }));
    expect(r).toEqual({ jobId: 'profile-job-2' });
    expect(waitForSampleJob).toHaveBeenCalledWith('sample-job-4');
    expect(waiting).toEqual([true, false]);
  });
  it('retries after a short pause when nothing is listed (job ended meanwhile)', async () => {
    const startBuild = vi.fn().mockResolvedValueOnce(busy).mockResolvedValueOnce({ jobId: 'profile-job-5' });
    const sleep = vi.fn(async () => undefined);
    expect(await resolveBuildJob(make({ startBuild, sleep }))).toEqual({ jobId: 'profile-job-5' });
    expect(sleep).toHaveBeenCalledTimes(1);
  });
  it('gives up with ENROLL_BUSY (shown, not swallowed) when it stays busy with nothing to follow', async () => {
    const startBuild = vi.fn(async () => busy);
    expect(await resolveBuildJob(make({ startBuild }))).toEqual({ errorCode: 'ENROLL_BUSY' });
    expect(startBuild.mock.calls.length).toBeLessThanOrEqual(4);
  });
  it('clears the waiting flag and rethrows when the sample wait is aborted', async () => {
    const waiting: boolean[] = [];
    await expect(resolveBuildJob(make({
      startBuild: async () => busy,
      activeJobs: async () => [{ jobId: 'sample-job-4', kind: 'sample', stage: 'trim' }],
      waitForSampleJob: async () => { throw new Error('aborted'); },
      onProcessingSample: (w) => waiting.push(w),
    }))).rejects.toThrow('aborted');
    expect(waiting).toEqual([true, false]);
  });
});
