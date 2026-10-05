import { describe, it, expect } from 'vitest';
import { renderToStaticMarkup as html } from 'react-dom/server';
import type { EnrollmentLabels, ServiceSample } from '../enrollmentTypes';
import { VoiceSampleGallery } from '../VoiceSampleGallery';
import { CallsPanel } from '../panels/CallsPanel';
import { ApprovalGuard } from '../hooks/useCallTakes';

const labels = { deleteAction: 'del', deleting: 'deleting', seconds: 's', usedInProfile: 'u', notUsed: 'n', otherMicrophone: 'o', needsReenroll: 'r', errors: {} } as unknown as EnrollmentLabels;
const sample: ServiceSample = { id: 'a', name: 'A', timestamp: '', speechSeconds: 3, deviceLabel: 'M', usedInProfile: true, needsReenroll: false, otherMicrophone: false };
const budget = { usedSeconds: 3, maxSeconds: 90, remainingSeconds: 87 };

describe('delete is locked while a profile build runs', () => {
  it('busy disables every delete button', () => {
    const busy = html(<VoiceSampleGallery samples={[sample]} budget={budget} labels={labels} onDelete={() => {}} busy />);
    expect(busy).toMatch(/<button[^>]*data-testid="delete-a"[^>]*disabled=""/);
    const idle = html(<VoiceSampleGallery samples={[sample]} budget={budget} labels={labels} onDelete={() => {}} />);
    expect(idle).not.toMatch(/<button[^>]*data-testid="delete-a"[^>]*disabled=""/);
  });
});

describe('approve take is guarded against double clicks', () => {
  it('ApprovalGuard lets one approval per id run at a time', () => {
    const g = new ApprovalGuard();
    expect(g.begin('t1')).toBe(true);
    expect(g.begin('t1')).toBe(false);
    expect(g.begin('t2')).toBe(true);
    g.end('t1');
    expect(g.begin('t1')).toBe(true);
  });
  it('CallsPanel disables approve for a take being approved', () => {
    const take = { id: 't1', timestamp: 'now' };
    const t = (k: string) => k;
    const m = html(<CallsPanel t={t} locale="pt-BR" takes={[take]} playingAudioId={null} errorText={null} approvingIds={['t1']} onPlay={() => {}} onApprove={() => {}} onDismiss={() => {}} />);
    expect(m).toMatch(/<button[^>]*class="action-btn take-approve-btn"[^>]*disabled=""/);
    const idle = html(<CallsPanel t={t} locale="pt-BR" takes={[take]} playingAudioId={null} errorText={null} onPlay={() => {}} onApprove={() => {}} onDismiss={() => {}} />);
    expect(idle).not.toMatch(/<button[^>]*class="action-btn take-approve-btn"[^>]*disabled=""/);
  });
});
