import { it, expect, vi, beforeEach } from 'vitest';
import type { EnrollmentJob, JobStage, JobState, CapturedPcm } from '../enrollmentTypes';

vi.mock('../../bridge', () => ({ invokeBridge: vi.fn() }));
import { invokeBridge } from '../../bridge';
import { addSample, buildProfile, listSamples, deleteSample, waitForJob } from '../enrollmentClient';

const mock = vi.mocked(invokeBridge);
beforeEach(() => mock.mockReset());

const job = (state: JobState, stage: JobStage): EnrollmentJob => ({
  jobId: 'job-1', state, stage, errorCode: null, remainingSeconds: null, sampleId: null, profileId: null, quality: null,
});
const captured: CapturedPcm = {
  pcm: new Float32Array([0.1, 0.2]), sampleRate: 48_000, durationSec: 2, peak: 0.2, rmsDbfs: -20,
  device: { label: 'Yeti', idHash: 'a'.repeat(64) },
};

it('waitForJob polls until done and reports updates', async () => {
  const seq: EnrollmentJob[] = [job('running', 'denoise'), job('running', 'trim'), job('done', 'apply')];
  const seen: string[] = [];
  const out = await waitForJob('job-1', { intervalMs: 1, getJob: async () => seq.shift()!, onUpdate: (j) => seen.push(j.stage) });
  expect(out.state).toBe('done'); expect(seen).toEqual(['denoise', 'trim', 'apply']);
});
it('waitForJob stops on a failed job and exposes the budget error with remaining seconds', async () => {
  const out = await waitForJob('job-1', { intervalMs: 1, getJob: async () => ({ ...job('failed', 'trim'), errorCode: 'ENROLL_BUDGET_EXCEEDED', remainingSeconds: 2.5 }) });
  expect(out.errorCode).toBe('ENROLL_BUDGET_EXCEEDED'); expect(out.remainingSeconds).toBe(2.5);
});
it('waitForJob times out', async () => { await expect(waitForJob('job-1', { intervalMs: 1, timeoutMs: 5, getJob: async () => job('running', 'queued') })).rejects.toThrow('timeout'); });
it('waitForJob turns an {errorCode} response into a synthetic failed job', async () => {
  const out = await waitForJob('job-9', { intervalMs: 1, getJob: async () => ({ errorCode: 'ENROLL_JOB_NOT_FOUND' as const }) });
  expect(out.state).toBe('failed'); expect(out.errorCode).toBe('ENROLL_JOB_NOT_FOUND'); expect(out.jobId).toBe('job-9');
});
it('waitForJob defaults to CMD.getJob via the bridge', async () => {
  mock.mockResolvedValueOnce(job('done', 'apply'));
  const out = await waitForJob('job-1', { intervalMs: 1 });
  expect(out.state).toBe('done'); expect(mock).toHaveBeenCalledWith('enrollment_get_job', { jobId: 'job-1' });
});
it('addSample sends the Float32Array as-is with sampleRate, name and device', async () => {
  mock.mockResolvedValueOnce({ jobId: 'j' });
  expect(await addSample(captured, 'Amostra')).toEqual({ jobId: 'j' });
  expect(mock).toHaveBeenCalledWith('enrollment_add_sample', { pcm: captured.pcm, sampleRate: 48_000, name: 'Amostra', device: captured.device });
  expect((mock.mock.calls[0][1] as { pcm: unknown }).pcm).toBe(captured.pcm);
});
it('addSample passes the error through', async () => {
  mock.mockResolvedValueOnce({ errorCode: 'ENROLL_BUDGET_EXCEEDED' });
  expect(await addSample(captured, 'n')).toEqual({ errorCode: 'ENROLL_BUDGET_EXCEEDED' });
});
it('buildProfile sends the name', async () => {
  mock.mockResolvedValueOnce({ jobId: 'p' });
  expect(await buildProfile('Eu')).toEqual({ jobId: 'p' });
  expect(mock).toHaveBeenCalledWith('enrollment_build_profile', { name: 'Eu' });
});
it('listSamples returns the list or the error', async () => {
  const list = { samples: [], budget: { usedSeconds: 0, maxSeconds: 90, remainingSeconds: 90 } };
  mock.mockResolvedValueOnce(list); expect(await listSamples()).toEqual(list);
  mock.mockResolvedValueOnce({ errorCode: 'SERVICE_UNAVAILABLE' }); expect(await listSamples()).toEqual({ errorCode: 'SERVICE_UNAVAILABLE' });
  expect(mock).toHaveBeenCalledWith('enrollment_list_samples', {});
});
it('deleteSample resolves true/false and false on error', async () => {
  mock.mockResolvedValueOnce({ success: true }); expect(await deleteSample('a')).toBe(true);
  expect(mock).toHaveBeenCalledWith('enrollment_delete_sample', { id: 'a' });
  mock.mockResolvedValueOnce({ success: false }); expect(await deleteSample('a')).toBe(false);
  mock.mockResolvedValueOnce({ errorCode: 'ENROLL_FAILED' }); expect(await deleteSample('a')).toBe(false);
});

const unavailable = { errorCode: 'SERVICE_UNAVAILABLE' as const };
const seqGet = (items: Array<EnrollmentJob | { errorCode: 'SERVICE_UNAVAILABLE' | 'ENROLL_JOB_NOT_FOUND' } | Error>) => {
  const fn = vi.fn(async () => { const x = items.shift()!; if (x instanceof Error) throw x; return x; });
  return fn;
};
it('waitForJob rejects aborted and stops polling when aborted mid-wait', async () => {
  const ac = new AbortController();
  const getJob = vi.fn(async () => job('running', 'queued'));
  const p = waitForJob('job-1', { intervalMs: 20, getJob, signal: ac.signal });
  await new Promise((r) => setTimeout(r, 5));
  ac.abort();
  await expect(p).rejects.toThrow('aborted');
  const n = getJob.mock.calls.length;
  await new Promise((r) => setTimeout(r, 60));
  expect(getJob.mock.calls.length).toBe(n);
});
it('waitForJob with an already aborted signal never polls', async () => {
  const ac = new AbortController(); ac.abort();
  const getJob = vi.fn(async () => job('done', 'apply'));
  await expect(waitForJob('job-1', { intervalMs: 1, getJob, signal: ac.signal })).rejects.toThrow('aborted');
  expect(getJob).not.toHaveBeenCalled();
});
it('waitForJob rides out one transient error', async () => {
  const out = await waitForJob('job-1', { intervalMs: 1, getJob: seqGet([unavailable, job('running', 'trim'), job('done', 'apply')]) });
  expect(out.state).toBe('done');
  const out2 = await waitForJob('job-1', { intervalMs: 1, getJob: seqGet([unavailable, job('done', 'apply')]) });
  expect(out2.state).toBe('done');
});
it('waitForJob rides out a thrown invoke error', async () => {
  const out = await waitForJob('job-1', { intervalMs: 1, getJob: seqGet([new Error('ipc'), job('done', 'apply')]) });
  expect(out.state).toBe('done');
});
it('waitForJob fails after 3 consecutive transient errors, and the counter resets on good responses', async () => {
  const out = await waitForJob('job-1', { intervalMs: 1, getJob: seqGet([unavailable, unavailable, unavailable]) });
  expect(out.state).toBe('failed'); expect(out.errorCode).toBe('SERVICE_UNAVAILABLE');
  const ok = await waitForJob('job-1', { intervalMs: 1, getJob: seqGet([unavailable, unavailable, job('running', 'trim'), unavailable, unavailable, job('done', 'apply')]) });
  expect(ok.state).toBe('done');
});
it('waitForJob fails immediately on ENROLL_JOB_NOT_FOUND', async () => {
  const getJob = seqGet([{ errorCode: 'ENROLL_JOB_NOT_FOUND' }, job('done', 'apply')]);
  const out = await waitForJob('job-1', { intervalMs: 1, getJob });
  expect(out.errorCode).toBe('ENROLL_JOB_NOT_FOUND'); expect(getJob).toHaveBeenCalledTimes(1);
});
