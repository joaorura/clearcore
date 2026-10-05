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
const MAX_TRANSIENT_FAILURES = 3;

function abortableSleep(ms: number, signal?: AbortSignal): Promise<void> {
  return new Promise<void>((resolve, reject) => {
    if (signal?.aborted) return reject(new Error('aborted'));
    const onAbort = () => { clearTimeout(t); reject(new Error('aborted')); };
    const t = setTimeout(() => { signal?.removeEventListener('abort', onAbort); resolve(); }, ms);
    signal?.addEventListener('abort', onAbort, { once: true });
  });
}

export async function waitForJob(
  jobId: string,
  o: { intervalMs?: number; timeoutMs?: number; onUpdate?: (j: EnrollmentJob) => void; getJob?: GetJob; signal?: AbortSignal } = {},
): Promise<EnrollmentJob> {
  const { intervalMs = 500, timeoutMs = 120_000, onUpdate, getJob = defaultGetJob, signal } = o;
  const deadline = Date.now() + timeoutMs;
  let transient = 0;
  for (;;) {
    if (signal?.aborted) throw new Error('aborted');
    let res: EnrollmentJob | { errorCode: EnrollErrorCode } | null = null;
    try {
      res = await getJob(jobId);
    } catch {
      res = { errorCode: 'SERVICE_UNAVAILABLE' };
    }
    if (signal?.aborted) throw new Error('aborted');
    let job: EnrollmentJob | null = null;
    if ('state' in res) {
      transient = 0;
      job = res;
    } else {
      const code = enrollmentErrorCode(res) ?? 'ENROLL_FAILED';
      if (code === 'SERVICE_UNAVAILABLE' && ++transient < MAX_TRANSIENT_FAILURES) {
        job = null;
      } else {
        job = { jobId, state: 'failed', stage: 'queued', errorCode: code, remainingSeconds: null, sampleId: null, profileId: null, quality: null };
      }
    }
    if (job) {
      onUpdate?.(job);
      if (job.state !== 'running') return job;
    }
    if (Date.now() + intervalMs > deadline) throw new Error('timeout');
    await abortableSleep(intervalMs, signal);
  }
}
