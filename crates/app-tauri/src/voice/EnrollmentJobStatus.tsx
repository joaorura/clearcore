import type { EnrollmentJob, EnrollmentLabels, JobStage } from './enrollmentTypes';
import { formatDecimalLocale, formatSecondsLocale } from './format';

const stageKey: Record<JobStage, keyof EnrollmentLabels> = {
  queued: 'stageQueued', denoise: 'stageDenoise', trim: 'stageTrim', eq: 'stageEq', enroll: 'stageEnroll', apply: 'stageApply', timeout: 'stageTimeout',
};

export function EnrollmentJobStatus({ job, labels, lang = 'en-US' }: { job: EnrollmentJob | null; labels: EnrollmentLabels; lang?: string }) {
  if (!job) return null;
  if (job.state === 'failed') {
    const code = job.errorCode ?? 'UNKNOWN';
    // The watchdog's timeout says what happened better than the generic ENROLL_FAILED.
    const detail = job.stage === 'timeout' ? labels.stageTimeout : (labels.errors[code] ?? labels.errors.UNKNOWN);
    return (
      <div role="alert" className="enrollment-job-status" style={{ color: '#f87171', fontSize: 13 }}>
        <strong>{labels.jobFailed}</strong>: {detail}
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
            <li>{labels.qualityPeak}: {formatDecimalLocale(q.peak, 2, lang)}</li>
            <li>{labels.qualityLevel}: {formatDecimalLocale(q.rmsDbfs, 1, lang)} dBFS</li>
            <li>{labels.qualitySpeech}: {formatSecondsLocale(q.speechSeconds, lang)} {labels.seconds}</li>
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
