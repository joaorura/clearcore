import { describe, it, expect } from 'vitest';
import { renderToStaticMarkup as html } from 'react-dom/server';
import type { EnrollmentLabels, ServiceSample } from '../enrollmentTypes';
import { VoiceSampleGallery } from '../VoiceSampleGallery';
import { CallsPanel } from '../panels/CallsPanel';
import { ApprovalGuard } from '../hooks/useCallTakes';
import { EnrollPanel } from '../panels/EnrollPanel';
import { ProfilePanel } from '../panels/ProfilePanel';

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

describe('build profile is guarded against multiple clicks and shows loading', () => {
  it('EnrollPanel activate-profile-master-btn is disabled and displays building text when isBuilding is true', () => {
    const t = (k: string) => k;
    const done = { 1: { duration: 3 }, 2: { duration: 3 }, 3: { duration: 3 }, 4: { duration: 3 }, 5: { duration: 3 } };
    const props = {
      t, locale: 'pt-BR', labels, isEnrolled: false, currentStep: 5, isReadingMode: false, completedSteps: done,
      isRecording: false, liveVoiceLevel: 0, recordingElapsedSeconds: 0, captureError: null, jobBusy: false,
      isBuilding: true, feedback: null, onSelectStep: () => {}, onToggleReadingMode: () => {}, onStartStep: () => {},
      onFinishStep: () => {}, onRedoStep: () => {}, onNextStep: () => {}, onBuildProfile: () => {}, onResetEnrollment: () => {},
    };
    const m = html(<EnrollPanel {...props} />);
    expect(m).toMatch(/<button[^>]*class="activate-profile-master-btn"[^>]*disabled=""[^>]*>⏳ voiceProfile.buildingProfile/);
  });

  it('ProfilePanel action button is disabled and displays building text when isBuilding is true', () => {
    const t = (k: string) => k;
    const props = {
      t, locale: 'pt-BR', labels, profileStatus: { is_enrolled: false, active_samples_count: 2 },
      samplesCount: 2, canBuild: true, busy: false, isBuilding: true, feedback: null, onBuildProfile: () => {},
    };
    const m = html(<ProfilePanel {...props} />);
    expect(m).toMatch(/<button[^>]*class="action-btn"[^>]*disabled=""[^>]*>⏳ voiceProfile.buildingProfile/);
  });
});
