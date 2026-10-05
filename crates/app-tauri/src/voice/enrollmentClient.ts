import { invokeBridge } from '../bridge';
import { CMD } from './enrollmentTypes';
import type { CapturedPcm, EnrollErrorCode, EnrollmentJob, SampleList } from './enrollmentTypes';
import { enrollmentErrorCode } from './enrollmentErrors';

type JobStart = { jobId: string } | { errorCode: EnrollErrorCode };
type GetJob = (id: string) => Promise<EnrollmentJob | { errorCode: EnrollErrorCode }>;

/** The Float32Array goes through as-is: the Electron main process does the base64 encoding. */
export async function addSample(c: CapturedPcm, name: string): Promise<JobStart> {
  return invokeBridge<JobStart>(CMD.addSample, { pcm: c.pcm, sampleRate: c.sampleRate, name, device: c.device });
}

export async function buildProfile(name: string): Promise<JobStart> {
  return invokeBridge<JobStart>(CMD.buildProfile, { name });
}

export async function listSamples(): Promise<SampleList | { errorCode: EnrollErrorCode }> {
  return invokeBridge<SampleList | { errorCode: EnrollErrorCode }>(CMD.listSamples, {});
}

export async function deleteSample(id: string): Promise<boolean> {
  const res = await invokeBridge<{ success?: boolean } | { errorCode: EnrollErrorCode }>(CMD.deleteSample, { id });
  return (res as { success?: boolean })?.success === true;
}

const defaultGetJob: GetJob = (jobId) => invokeBridge(CMD.getJob, { jobId });
const sleep = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));

export async function waitForJob(
  jobId: string,
  o: { intervalMs?: number; timeoutMs?: number; onUpdate?: (j: EnrollmentJob) => void; getJob?: GetJob } = {},
): Promise<EnrollmentJob> {
  const { intervalMs = 500, timeoutMs = 120_000, onUpdate, getJob = defaultGetJob } = o;
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    const res = await getJob(jobId);
    const err = enrollmentErrorCode(res);
    const job: EnrollmentJob = 'state' in res
      ? res
      : { jobId, state: 'failed', stage: 'queued', errorCode: err ?? 'ENROLL_FAILED', remainingSeconds: null, sampleId: null, profileId: null, quality: null };
    onUpdate?.(job);
    if (job.state !== 'running') return job;
    if (Date.now() + intervalMs > deadline) throw new Error('timeout');
    await sleep(intervalMs);
  }
}
