import { describe, it, expect } from 'vitest';
import { renderToStaticMarkup as html } from 'react-dom/server';
import type { EnrollmentLabels, EnrollmentJob, ServiceSample, SpeechBudget, EnrollErrorCode } from '../enrollmentTypes';
import { VoiceBudgetMeter } from '../VoiceBudgetMeter';
import { VoiceSampleGallery } from '../VoiceSampleGallery';
import { BudgetErrorBanner } from '../BudgetErrorBanner';
import { EnrollmentJobStatus } from '../EnrollmentJobStatus';
import { DevModelNotice } from '../DevModelNotice';

const codes: Array<EnrollErrorCode | 'UNKNOWN'> = [
  'ENROLL_CLIPPING', 'ENROLL_TOO_QUIET', 'ENROLL_TOO_LITTLE_SPEECH', 'ENROLL_MODEL_NOT_CONFIGURED',
  'ENROLL_BUDGET_EXCEEDED', 'ENROLL_INVALID_AUDIO', 'ENROLL_PAYLOAD_TOO_LARGE',
  'ENROLL_JOB_NOT_FOUND', 'ENROLL_BUSY', 'ENROLL_FAILED', 'SERVICE_UNAVAILABLE', 'SERVICE_OUTDATED', 'UNKNOWN',
];
function makeLabels(): EnrollmentLabels {
  const l: Record<string, unknown> = {};
  for (const k of ['budgetTitle', 'budgetUsed', 'budgetRemaining', 'seconds', 'budgetExceededTitle', 'budgetExceededBody',
    'deleteAction', 'deleting', 'otherMicrophone', 'needsReenroll', 'usedInProfile', 'notUsed',
    'stageQueued', 'stageDenoise', 'stageTrim', 'stageEq', 'stageEnroll', 'stageApply', 'stageTimeout', 'jobDone', 'jobFailed',
    'devModelNotice', 'devIsolationModelNotice', 'qualityPeak', 'qualityLevel', 'qualitySpeech']) l[k] = k;
  l.devIsolationModelError = 'devIsolationModelError({code})';
  l.errors = Object.fromEntries(codes.map((c) => [c, c]));
  return l as unknown as EnrollmentLabels;
}
const labels = makeLabels();
const b0: SpeechBudget = { usedSeconds: 4.2, maxSeconds: 90, remainingSeconds: 85.8 };
const sA: ServiceSample = { id: 'a', name: 'A', timestamp: '1', speechSeconds: 4.2, deviceLabel: 'Yeti', usedInProfile: true, needsReenroll: false, otherMicrophone: false };
const sB: ServiceSample = { id: 'b', name: 'B', timestamp: '2', speechSeconds: 0, deviceLabel: 'Old', usedInProfile: false, needsReenroll: true, otherMicrophone: true };
const job = (o: Partial<EnrollmentJob>): EnrollmentJob => ({
  jobId: 'j', state: 'running', stage: 'queued', errorCode: null, remainingSeconds: null, sampleId: null, profileId: null, quality: null, ...o,
});

describe('components', () => {
  it('meter shows used and max and clamps the bar at 100%', () => {
    const m = html(<VoiceBudgetMeter budget={{ usedSeconds: 120, maxSeconds: 90, remainingSeconds: 0 }} labels={labels} />);
    expect(m).toContain('90'); expect(m).toContain('width:100%');
  });
  it('meter clamps at 0% for empty or invalid max', () => {
    expect(html(<VoiceBudgetMeter budget={{ usedSeconds: 0, maxSeconds: 90, remainingSeconds: 90 }} labels={labels} />)).toContain('width:0%');
    expect(html(<VoiceBudgetMeter budget={{ usedSeconds: 5, maxSeconds: 0, remainingSeconds: 0 }} labels={labels} />)).toContain('width:0%');
  });
  it('gallery flags other-microphone and re-record samples and offers delete on each row', () => {
    const m = html(<VoiceSampleGallery labels={labels} budget={b0} onDelete={() => {}} samples={[sA, sB]} />);
    expect(m).toContain('delete-a'); expect(m).toContain('delete-b');
    expect(m).toContain('otherMicrophone'); expect(m).toContain('needsReenroll'); expect(m).toContain('4.2');
    expect(m).toContain('usedInProfile'); expect(m).toContain('notUsed');
  });
  it('gallery emphasises delete with highlightDelete only', () => {
    const on = html(<VoiceSampleGallery labels={labels} budget={b0} onDelete={() => {}} samples={[sA]} highlightDelete />);
    const off = html(<VoiceSampleGallery labels={labels} budget={b0} onDelete={() => {}} samples={[sA]} />);
    expect(on).toContain('data-highlight="true"');
    expect(off).not.toContain('data-highlight="true"');
  });
  it('gallery disables the deleting row and shows the deleting label', () => {
    const m = html(<VoiceSampleGallery labels={labels} budget={b0} onDelete={() => {}} samples={[sA]} deletingId="a" />);
    expect(m).toContain('deleting'); expect(m).toContain('disabled');
  });
  it('a needsReenroll sample has no usage bar', () => {
    const m = html(<VoiceSampleGallery labels={labels} budget={b0} onDelete={() => {}} samples={[sB]} />);
    expect(m).not.toContain('sample-usage-bar');
    expect(html(<VoiceSampleGallery labels={labels} budget={b0} onDelete={() => {}} samples={[sA]} />)).toContain('sample-usage-bar');
  });
  it('budget banner interpolates the remaining seconds and prompts to delete', () => {
    const m = html(<BudgetErrorBanner remainingSeconds={2.5} labels={{ ...labels, budgetExceededBody: 'Restam {remaining} s' }} />);
    expect(m).toContain('Restam 2.5 s'); expect(m).toContain('role="alert"'); expect(m).toContain('budgetExceededTitle');
  });
  it('budget banner drops the remaining clause gracefully when null', () => {
    const m = html(<BudgetErrorBanner remainingSeconds={null} labels={{ ...labels, budgetExceededBody: 'Restam {remaining} s. Apague algo.' }} />);
    expect(m).not.toContain('{remaining}'); expect(m).toContain('Apague algo.'); expect(m).not.toContain('Restam');
  });
  it('job status maps stages to labels', () => {
    expect(html(<EnrollmentJobStatus job={job({ stage: 'trim' })} labels={labels} />)).toContain('stageTrim');
    expect(html(<EnrollmentJobStatus job={job({ stage: 'apply' })} labels={labels} />)).toContain('stageApply');
  });
  it('job status shows the error label, never raw text', () => {
    const m = html(<EnrollmentJobStatus job={job({ state: 'failed', errorCode: 'ENROLL_TOO_QUIET' })} labels={labels} />);
    expect(m).toContain('ENROLL_TOO_QUIET'); expect(m).toContain('jobFailed'); expect(m).toContain('role="alert"');
    const u = html(<EnrollmentJobStatus job={job({ state: 'failed', errorCode: null })} labels={labels} />);
    expect(u).toContain('UNKNOWN');
  });
  it('job status shows quality on success', () => {
    const m = html(<EnrollmentJobStatus labels={labels} job={job({ state: 'done', stage: 'apply', quality: { peak: 0.5, rmsDbfs: -23.46, activeFraction: 0.8, speechSeconds: 4.24 } })} />);
    expect(m).toContain('jobDone'); expect(m).toContain('qualityPeak'); expect(m).toContain('qualityLevel'); expect(m).toContain('qualitySpeech');
    expect(m).toContain('-23.5'); expect(m).toContain('4.2');
  });
  it('null job renders nothing', () => {
    expect(html(<EnrollmentJobStatus job={null} labels={labels} />)).toBe('');
  });
  it('dev model notice renders the label', () => {
    expect(html(<DevModelNotice labels={labels} />)).toContain('devModelNotice');
  });
  it('dev model notice names the development pDFNet3 only when the service says so', () => {
    const dev = html(<DevModelNotice labels={labels} devBaseModel="pdfnet3-dev" />);
    expect(dev).toContain('devModelNotice');
    expect(dev).toContain('devIsolationModelNotice');
    for (const m of [html(<DevModelNotice labels={labels} devBaseModel="base" />), html(<DevModelNotice labels={labels} />)]) {
      expect(m).not.toContain('devIsolationModelNotice');
      expect(m).not.toContain('devIsolationModelError');
    }
  });
  it('dev model notice shows the fixed code when the development model was not loaded', () => {
    const m = html(<DevModelNotice labels={labels} devBaseModel="base" devBaseModelError="DEV_MODEL_HASH_MISMATCH" />);
    expect(m).toContain('devIsolationModelError(DEV_MODEL_HASH_MISMATCH)');
    expect(m).not.toContain('devIsolationModelNotice');
  });
});

describe('EnrollmentJobStatus timeout (M6)', () => {
  it('a job failed by the watchdog shows the timeout, never "queued"', () => {
    const m = html(<EnrollmentJobStatus job={job({ state: 'failed', stage: 'timeout', errorCode: 'ENROLL_FAILED' })} labels={labels} />);
    expect(m).toContain('jobFailed');
    expect(m).toContain('stageTimeout');
    expect(m).not.toContain('stageQueued');
  });
});
