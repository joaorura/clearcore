import type { EnrollmentJob, EnrollmentLabels, JobStage } from './enrollmentTypes';

const stageKey: Record<JobStage, keyof EnrollmentLabels> = {
  queued: 'stageQueued', denoise: 'stageDenoise', trim: 'stageTrim', eq: 'stageEq', enroll: 'stageEnroll', apply: 'stageApply',
};

const one = (n: number) => (Math.round(n * 10) / 10).toFixed(1);

export function EnrollmentJobStatus({ job, labels }: { job: EnrollmentJob | null; labels: EnrollmentLabels }) {
  if (!job) return null;
  if (job.state === 'failed') {
    const code = job.errorCode ?? 'UNKNOWN';
    return (
      <div role="alert" className="enrollment-job-status" style={{ color: '#f87171', fontSize: 13 }}>
        <strong>{labels.jobFailed}</strong>: {labels.errors[code] ?? labels.errors.UNKNOWN}
      </div>
    );
  }
  if (job.state === 'done') {
    const q = job.quality;
    return (
      <div role="status" className="enrollment-job-status" style={{ fontSize: 13 }}>
        <strong style={{ color: '#4ade80' }}>{labels.jobDone}</strong>
        {q && (
          <ul style={{ listStyle: 'none', margin: '4px 0 0', padding: 0, color: 'var(--text-muted)' }}>
            <li>{labels.qualityPeak}: {q.peak.toFixed(2)}</li>
            <li>{labels.qualityLevel}: {one(q.rmsDbfs)} dBFS</li>
            <li>{labels.qualitySpeech}: {one(q.speechSeconds)} {labels.seconds}</li>
          </ul>
        )}
      </div>
    );
  }
  return (
    <div role="status" aria-live="polite" className="enrollment-job-status" style={{ fontSize: 13, color: 'var(--text-muted)' }}>
      {labels[stageKey[job.stage]] as string}
    </div>
  );
}
