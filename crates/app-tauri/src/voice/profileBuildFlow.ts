import type { EnrollErrorCode } from './enrollmentTypes';
import { enrollmentErrorCode } from './enrollmentErrors';

/** A job the daemon is running right now, as GetStatus reports it (`active_enrollment_jobs`). */
export interface ActiveEnrollmentJob {
  jobId: string;
  kind: 'sample' | 'profile';
  stage: string;
}

/** Absent or malformed `active_enrollment_jobs` (older daemon, nothing running) means no job. */
export function normalizeActiveJobs(raw: unknown): ActiveEnrollmentJob[] {
  if (!Array.isArray(raw)) return [];
  const out: ActiveEnrollmentJob[] = [];
  for (const item of raw) {
    if (!item || typeof item !== 'object') continue;
    const j = item as { job_id?: unknown; kind?: unknown; stage?: unknown };
    if (typeof j.job_id !== 'string' || j.job_id === '') continue;
    if (j.kind !== 'sample' && j.kind !== 'profile') continue;
    out.push({ jobId: j.job_id, kind: j.kind, stage: typeof j.stage === 'string' ? j.stage : '' });
  }
  return out;
}

type JobStart = { jobId: string } | { errorCode: EnrollErrorCode };

export interface BuildFlowDeps {
  startBuild: () => Promise<JobStart>;
  activeJobs: () => Promise<ActiveEnrollmentJob[]>;
  /** Follows a sample job to its end (its outcome is irrelevant here; abort/timeout propagate). */
  waitForSampleJob: (jobId: string) => Promise<unknown>;
  sleep: (ms: number) => Promise<void>;
  /** True while the build waits for a running sample job to finish. */
  onProcessingSample?: (waiting: boolean) => void;
}

const MAX_BUSY_ATTEMPTS = 4;
const BUSY_RETRY_MS = 500;

/**
 * Starts a profile build and returns the job to follow. ENROLL_BUSY is not shown as an error: the
 * status says what is running, so a running profile build is followed (it is the same work the
 * user asked for) and a running sample is waited for before asking again.
 */
export async function resolveBuildJob(deps: BuildFlowDeps): Promise<{ jobId: string } | { errorCode: EnrollErrorCode }> {
  for (let attempt = 0; attempt < MAX_BUSY_ATTEMPTS; attempt++) {
    const start = await deps.startBuild();
    const code = enrollmentErrorCode(start);
    if (code === null && 'jobId' in start) return { jobId: start.jobId };
    if (code !== 'ENROLL_BUSY') return { errorCode: code ?? 'ENROLL_FAILED' };

    const active = await deps.activeJobs();
    const profileJob = active.find((j) => j.kind === 'profile');
    if (profileJob) return { jobId: profileJob.jobId };
    const sampleJob = active.find((j) => j.kind === 'sample');
    if (sampleJob) {
      deps.onProcessingSample?.(true);
      try {
        await deps.waitForSampleJob(sampleJob.jobId);
      } finally {
        deps.onProcessingSample?.(false);
      }
    } else {
      await deps.sleep(BUSY_RETRY_MS); // finished between the request and the status read
    }
  }
  return { errorCode: 'ENROLL_BUSY' };
}
