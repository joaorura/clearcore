import { describe, it, expect, vi } from 'vitest';
import { renderToStaticMarkup as html } from 'react-dom/server';
import type { EnrollmentJob, EnrollmentLabels, SampleList } from '../enrollmentTypes';
import type { CallSuggestionTake, VoiceProfileStatus } from '../../types';
import { EnrollPanel, type EnrollPanelProps } from '../panels/EnrollPanel';
import { GalleryPanel, type GalleryPanelProps } from '../panels/GalleryPanel';
import { CallsPanel, type CallsPanelProps } from '../panels/CallsPanel';
import { ProfilePanel, type ProfilePanelProps } from '../panels/ProfilePanel';
import { AddSampleModal, type AddSampleModalProps } from '../panels/AddSampleModal';
import { profileStatusLabel } from '../panels/profileStatusLabel';
import { jobFeedbackFor } from '../panels/voiceTabs';

/** Returns the key plus its params (k=v), so the markup shows exactly what was asked for. */
const t = (path: string, params?: Record<string, string | number>) =>
  params ? `${path}(${Object.entries(params).map(([k, v]) => `${k}=${v}`).join(';')})` : path;

const LABEL_KEYS = ['budgetTitle', 'budgetUsed', 'budgetRemaining', 'seconds', 'budgetExceededTitle', 'budgetExceededBody',
  'deleteAction', 'deleting', 'otherMicrophone', 'needsReenroll', 'usedInProfile', 'notUsed',
  'stageQueued', 'stageDenoise', 'stageTrim', 'stageEq', 'stageEnroll', 'stageApply', 'jobDone', 'jobFailed',
  'devModelNotice', 'qualityPeak', 'qualityLevel', 'qualitySpeech'];
const ERROR_CODES = ['ENROLL_CLIPPING', 'ENROLL_TOO_QUIET', 'ENROLL_TOO_LITTLE_SPEECH', 'ENROLL_MODEL_NOT_CONFIGURED',
  'ENROLL_BUDGET_EXCEEDED', 'ENROLL_INVALID_AUDIO', 'ENROLL_PAYLOAD_TOO_LARGE', 'ENROLL_JOB_NOT_FOUND', 'ENROLL_BUSY', 'ENROLL_FAILED',
  'SERVICE_UNAVAILABLE', 'ENROLL_BACKEND_UNSUPPORTED', 'UNKNOWN'];
const labels = {
  ...Object.fromEntries(LABEL_KEYS.map((k) => [k, k])),
  errors: Object.fromEntries(ERROR_CODES.map((c) => [c, `err:${c}`])),
} as unknown as EnrollmentLabels;

const noop = () => {};
const captured = { pcm: new Float32Array(4), sampleRate: 48_000 as const, durationSec: 3.25, peak: 0.4, rmsDbfs: -20, device: { label: 'Yeti', idHash: 'a'.repeat(64) } };
const runningJob: EnrollmentJob = { jobId: 'j', state: 'running', stage: 'trim', errorCode: null, remainingSeconds: null, sampleId: null, profileId: null, quality: null };

const enrollProps = (over: Partial<EnrollPanelProps> = {}): EnrollPanelProps => ({
  t, locale: 'pt-BR', labels,
  isEnrolled: false, currentStep: 1, isReadingMode: false, completedSteps: {},
  isRecording: false, liveVoiceLevel: 0, recordingElapsedSeconds: 0, captureError: null,
  jobBusy: false, feedback: null,
  onSelectStep: noop, onToggleReadingMode: noop, onStartStep: noop, onFinishStep: noop, onRedoStep: noop,
  onNextStep: noop, onBuildProfile: noop, onResetEnrollment: noop,
  ...over,
});

describe('EnrollPanel', () => {
  it('shows the current question and the record button when idle', () => {
    const m = html(<EnrollPanel {...enrollProps({ currentStep: 2 })} />);
    expect(m).toContain('voiceProfile.question2Category');
    expect(m).toContain('voiceProfile.question2Text');
    expect(m).toContain('voiceProfile.recordSample');
    expect(m).not.toContain('voiceProfile.activateProfileBtn');
  });
  it('reading mode shows the fallback sentence', () => {
    expect(html(<EnrollPanel {...enrollProps({ isReadingMode: true })} />)).toContain('voiceProfile.question1Fallback');
  });
  it('while recording shows the real level and the elapsed time with a decimal comma, stop disabled below the minimum', () => {
    const m = html(<EnrollPanel {...enrollProps({ isRecording: true, liveVoiceLevel: 37, recordingElapsedSeconds: 1.2 })} />);
    expect(m).toContain('37%');
    expect(m).toContain('elapsed=1,2;');
    expect(m).toMatch(/<button[^>]*class="stop-record-btn"[^>]*disabled=""/);
    const ok = html(<EnrollPanel {...enrollProps({ isRecording: true, recordingElapsedSeconds: 2 })} />);
    expect(ok).not.toMatch(/<button[^>]*class="stop-record-btn"[^>]*disabled=""/);
  });
  it('a completed step offers redo and next (no "Ouvir": the sent PCM was zeroed); all five offer the profile build', () => {
    const done = { 1: { duration: 3 }, 2: { duration: 3 }, 3: { duration: 3 }, 4: { duration: 3 }, 5: { duration: 3 } };
    const m = html(<EnrollPanel {...enrollProps({ completedSteps: { 1: done[1] } })} />);
    expect(m).not.toContain('voiceProfile.playSample'); expect(m).toContain('voiceProfile.redoSample'); expect(m).toContain('voiceProfile.nextStep');
    expect(m).not.toContain('voiceProfile.activateProfileBtn');
    expect(html(<EnrollPanel {...enrollProps({ completedSteps: done, currentStep: 5 })} />)).toContain('voiceProfile.activateProfileBtn');
  });
  it('an enrolled profile offers the full re-enrollment and hides the finished flow', () => {
    const done = { 1: { duration: 3 }, 2: { duration: 3 }, 3: { duration: 3 }, 4: { duration: 3 }, 5: { duration: 3 } };
    const m = html(<EnrollPanel {...enrollProps({ isEnrolled: true, completedSteps: done })} />);
    expect(m).toContain('voiceProfile.reEnrollBtn');
    expect(m).not.toContain('voiceProfile.recordSample');
    expect(html(<EnrollPanel {...enrollProps()} />)).not.toContain('voiceProfile.reEnrollBtn');
  });
  it('shows the capture error and the sample job feedback', () => {
    const m = html(<EnrollPanel {...enrollProps({ captureError: 'no-mic', feedback: { jobBusy: true, currentJob: runningJob, enrollErrorText: 'bad' } })} />);
    expect(m).toContain('no-mic'); expect(m).toContain('stageTrim'); expect(m).toContain('bad');
  });
});

const list: SampleList = {
  samples: [{ id: 's1', name: 'Um', timestamp: '1', speechSeconds: 4.25, deviceLabel: 'Yeti', usedInProfile: true, needsReenroll: false, otherMicrophone: false }],
  budget: { usedSeconds: 88.5, maxSeconds: 90, remainingSeconds: 1.5 },
};
const galleryProps = (over: Partial<GalleryPanelProps> = {}): GalleryPanelProps => ({
  t, locale: 'pt-BR', labels, sampleList: list, samplesLoadFailed: false, deletingId: null, budgetError: null, feedback: null,
  onDelete: noop, onAddSample: noop, ...over,
});

describe('GalleryPanel', () => {
  it('shows the meter, each sample with its delete action and the add button, in pt-BR decimals', () => {
    const m = html(<GalleryPanel {...galleryProps()} />);
    expect(m).toContain('voice-budget-meter'); expect(m).toContain('88,5'); expect(m).toContain('4,3');
    expect(m).toContain('delete-s1'); expect(m).toContain('voiceProfile.addNewSampleBtn');
    expect(m).not.toContain('data-highlight="true"'); expect(m).not.toContain('budgetExceededTitle');
  });
  it('on the budget error shows the banner with the localized remaining seconds and highlights delete', () => {
    const m = html(<GalleryPanel {...galleryProps({ labels: { ...labels, budgetExceededBody: 'Restam {remaining} s.' }, budgetError: { remainingSeconds: 1.5 } })} />);
    expect(m).toContain('budgetExceededTitle'); expect(m).toContain('Restam 1,5 s.'); expect(m).toContain('data-highlight="true"');
  });
  it('empty and failed states', () => {
    expect(html(<GalleryPanel {...galleryProps({ sampleList: null })} />)).toContain('voiceProfile.emptyGallery');
    expect(html(<GalleryPanel {...galleryProps({ sampleList: null, samplesLoadFailed: true })} />)).toContain('voiceProfile.samplesLoadFailed');
  });
});

const take: CallSuggestionTake = { id: 'k1', title: 'Reunião', timestamp: '10:00', speech_seconds: 6.25, snrDb: 18.04, device_label: 'Yeti' };
const callsProps = (over: Partial<CallsPanelProps> = {}): CallsPanelProps => ({
  t, locale: 'pt-BR', takes: [take], playingAudioId: null, errorText: null, onPlay: noop, onApprove: noop, onDismiss: noop, ...over,
});

describe('CallsPanel', () => {
  it('lists the service takes with approve and dismiss', () => {
    const m = html(<CallsPanel {...callsProps()} />);
    expect(m).toContain('Reunião'); expect(m).toContain('voiceProfile.approveTake'); expect(m).toContain('voiceProfile.dismissTake');
    expect(m).toContain('sec=6,3'); expect(m).toContain('snr=18,0');
  });
  it('offers play only when the service gave an audio url', () => {
    expect(html(<CallsPanel {...callsProps()} />)).not.toContain('take-play-btn');
    expect(html(<CallsPanel {...callsProps({ takes: [{ ...take, audioUrl: 'file:///x.wav' }] })} />)).toContain('take-play-btn');
  });
  it('empty state and approval error', () => {
    expect(html(<CallsPanel {...callsProps({ takes: [] })} />)).toContain('voiceProfile.emptyCallSuggestions');
    expect(html(<CallsPanel {...callsProps({ errorText: 'falhou' })} />)).toMatch(/role="alert"[^>]*>falhou/);
  });
});

const profileProps = (over: Partial<ProfilePanelProps> = {}): ProfilePanelProps => ({
  t, labels, locale: 'pt-BR', profileStatus: { is_enrolled: false, active_samples_count: 0 }, samplesCount: 2,
  canBuild: true, busy: false, feedback: null, onBuildProfile: noop, ...over,
});

describe('ProfilePanel', () => {
  it('leaves the development model notice to the card header (no duplicate)', () => {
    expect(html(<ProfilePanel {...profileProps()} />)).not.toContain('devModelNotice');
  });
  it('active only when the service confirms, with the applied-in-service note', () => {
    const active: VoiceProfileStatus = { is_enrolled: true, active_samples_count: 2, is_voice_profile_active: true };
    const m = html(<ProfilePanel {...profileProps({ profileStatus: active })} />);
    expect(m).toContain('voiceProfile.statusActive'); expect(m).toContain('voiceProfile.appliedInServiceNote');
  });
  it('stored but not applied shows the translated service error, never its text', () => {
    const st: VoiceProfileStatus = { is_enrolled: true, active_samples_count: 2, stored_voice_profile_id: 'p1', is_voice_profile_active: false, voice_profile_error: 'service_unavailable' };
    const m = html(<ProfilePanel {...profileProps({ profileStatus: st })} />);
    expect(m).toContain('voiceProfile.storedNotApplied'); expect(m).toContain('voiceProfile.errorServiceUnavailable');
    expect(m).not.toContain('voiceProfile.appliedInServiceNote');
    const raw = html(<ProfilePanel {...profileProps({ profileStatus: { ...st, voice_profile_error: '<b>raw</b>' } })} />);
    expect(raw).toContain('voiceProfile.errorUnknown'); expect(raw).not.toContain('raw');
  });
  it('build button: generate before enrollment, rebuild after; hidden with no samples; disabled when busy', () => {
    expect(html(<ProfilePanel {...profileProps()} />)).toContain('voiceProfile.activateProfileBtn');
    expect(html(<ProfilePanel {...profileProps({ profileStatus: { is_enrolled: true, active_samples_count: 2, stored_voice_profile_id: 'p1' } })} />)).toContain('voiceProfile.rebuildProfileBtn');
    // A local is_enrolled alone (old flow) is no profile: the button still generates one.
    expect(html(<ProfilePanel {...profileProps({ profileStatus: { is_enrolled: true, active_samples_count: 2 } })} />)).toContain('voiceProfile.activateProfileBtn');
    const none = html(<ProfilePanel {...profileProps({ canBuild: false })} />);
    expect(none).not.toContain('voiceProfile.activateProfileBtn'); expect(none).not.toContain('voiceProfile.rebuildProfileBtn');
    expect(html(<ProfilePanel {...profileProps({ busy: true })} />)).toMatch(/<button[^>]*disabled=""[^>]*>voiceProfile.activateProfileBtn/);
  });
  it('shows the build job stage and the sample count', () => {
    const m = html(<ProfilePanel {...profileProps({ feedback: { jobBusy: true, currentJob: { ...runningJob, stage: 'enroll' }, enrollErrorText: null } })} />);
    expect(m).toContain('stageEnroll'); expect(m).toContain('voiceProfile.statusSamplesPill(count=2)');
  });
  it('voice_profile_supported false: notice shown and the build button disabled with the reason', () => {
    const st: VoiceProfileStatus = { is_enrolled: false, active_samples_count: 2, voice_profile_supported: false };
    const m = html(<ProfilePanel {...profileProps({ profileStatus: st })} />);
    expect(m).toContain('voiceProfile.profileUnsupportedNotice');
    expect(m).toMatch(/<button[^>]*disabled=""[^>]*title="voiceProfile.profileUnsupportedNotice"[^>]*aria-describedby="voice-profile-unsupported"[^>]*>voiceProfile.activateProfileBtn/);
    expect(m).toContain('id="voice-profile-unsupported"');
    // The rest of the tab stays: status, samples count and EQ.
    expect(m).toContain('voiceProfile.statusSamplesPill(count=2)');
  });
  it('voice_profile_supported true or absent (older service): no notice, button enabled', () => {
    for (const st of [
      { is_enrolled: false, active_samples_count: 2, voice_profile_supported: true },
      { is_enrolled: false, active_samples_count: 2 },
    ] as VoiceProfileStatus[]) {
      const m = html(<ProfilePanel {...profileProps({ profileStatus: st })} />);
      expect(m).not.toContain('voiceProfile.profileUnsupportedNotice');
      expect(m).not.toMatch(/<button[^>]*disabled=""/);
      expect(m).toContain('voiceProfile.activateProfileBtn');
    }
  });
  it('neural EQ shows calibrated only when the service says so', () => {
    expect(html(<ProfilePanel {...profileProps()} />)).toContain('voiceProfile.neuralEqPending');
    // As the service would send it (the renderer never sets this field itself).
    const fromService = JSON.parse('{"is_enrolled":true,"active_samples_count":1,"neural_eq_calibrated":true}') as VoiceProfileStatus;
    expect(html(<ProfilePanel {...profileProps({ profileStatus: fromService })} />)).toContain('voiceProfile.neuralEqCalibrated');
  });
});

describe('profileStatusLabel', () => {
  it('maps the status to its label and activity', () => {
    expect(profileStatusLabel({ is_enrolled: false, active_samples_count: 0 }, t)).toEqual({ text: 'voiceProfile.statusPending', active: false });
    expect(profileStatusLabel({ is_enrolled: true, active_samples_count: 0 }, t)).toEqual({ text: 'voiceProfile.statusPending', active: false });
    expect(profileStatusLabel({ is_enrolled: false, active_samples_count: 0, has_voice_profile: true }, t)).toEqual({ text: 'voiceProfile.storedNotApplied', active: false });
    expect(profileStatusLabel({ is_enrolled: true, active_samples_count: 0, is_voice_profile_active: true }, t)).toEqual({ text: 'voiceProfile.statusActive', active: true });
  });
});

describe('AddSampleModal', () => {
  const props = (over: Partial<AddSampleModalProps> = {}): AddSampleModalProps => ({
    t, locale: 'pt-BR', labels, name: '', onNameChange: noop, captured: null, isRecording: false, liveVoiceLevel: 0,
    recordingElapsedSeconds: 0, captureError: null, jobBusy: false, feedback: null, playingAudioId: null,
    onStart: noop, onStop: noop, onPlayPreview: noop, onCancel: noop, onSave: noop, ...over,
  });
  it('save is disabled until a take is captured; the preview shows its duration with a comma', () => {
    expect(html(<AddSampleModal {...props()} />)).toMatch(/<button[^>]*disabled=""[^>]*>voiceProfile.modalSave/);
    const m = html(<AddSampleModal {...props({ captured })} />);
    expect(m).not.toMatch(/<button[^>]*disabled=""[^>]*>voiceProfile.modalSave/);
    expect(m).toContain('sec=3,3'); expect(m).toContain('voiceProfile.redoSample');
  });
  it('shows the capture error and the job feedback inside the modal', () => {
    const onSave = vi.fn();
    const m = html(<AddSampleModal {...props({ onSave, captureError: 'no-mic', feedback: { jobBusy: true, currentJob: null, enrollErrorText: null } })} />);
    expect(m).toContain('no-mic'); expect(m).toContain('voiceProfile.sendingSample');
  });
});

describe('profile build feedback follows the tab that started it', () => {
  const failed: EnrollmentJob = { ...runningJob, state: 'failed', stage: 'enroll', errorCode: 'ENROLL_TOO_LITTLE_SPEECH' };
  const view = { jobBusy: false, currentJob: failed, enrollErrorText: 'err:ENROLL_FAILED' };
  const running = { jobBusy: true, currentJob: { ...runningJob, stage: 'enroll' as const }, enrollErrorText: null };
  it('started on Enrollment: Enrollment shows the stage and the errors', () => {
    const src = { origin: 'enroll', kind: 'build' } as const;
    expect(html(<EnrollPanel {...enrollProps({ feedback: jobFeedbackFor('enroll', src, running) })} />)).toContain('stageEnroll');
    const m = html(<EnrollPanel {...enrollProps({ feedback: jobFeedbackFor('enroll', src, view) })} />);
    expect(m).toContain('err:ENROLL_TOO_LITTLE_SPEECH'); expect(m).toContain('err:ENROLL_FAILED');
  });
  it('started on Enrollment: Profile shows them too', () => {
    const src = { origin: 'enroll', kind: 'build' } as const;
    const m = html(<ProfilePanel {...profileProps({ feedback: jobFeedbackFor('profile', src, view) })} />);
    expect(m).toContain('err:ENROLL_TOO_LITTLE_SPEECH'); expect(m).toContain('err:ENROLL_FAILED');
  });
  it('started on Profile: Profile shows the stage', () => {
    const src = { origin: 'profile', kind: 'build' } as const;
    expect(html(<ProfilePanel {...profileProps({ feedback: jobFeedbackFor('profile', src, running) })} />)).toContain('stageEnroll');
  });
});

describe('EnrollPanel stepper', () => {
  it('segments are labelled buttons, disabled while recording', () => {
    const idle = html(<EnrollPanel {...enrollProps()} />);
    expect((idle.match(/<button[^>]*class="stepper-segment[^"]*"[^>]*aria-label="voiceProfile.stepTitle\(n=\d\)"/g) ?? []).length).toBe(5);
    const rec = html(<EnrollPanel {...enrollProps({ isRecording: true })} />);
    expect((rec.match(/<button[^>]*class="stepper-segment[^"]*"[^>]*disabled=""/g) ?? []).length).toBe(5);
    expect((idle.match(/<button[^>]*class="stepper-segment[^"]*"[^>]*disabled=""/g) ?? []).length).toBe(0);
  });
});
