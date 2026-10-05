import { describe, it, expect } from 'vitest';
import { renderToStaticMarkup as html } from 'react-dom/server';
import type { EnrollmentLabels } from '../enrollmentTypes';
import { EnrollPanel, type EnrollPanelProps } from '../panels/EnrollPanel';
import { AddSampleModal, type AddSampleModalProps } from '../panels/AddSampleModal';
import { shouldReportUnstartedCapture } from '../hooks/voiceProfileLogic';

const t = (path: string) => path;
const labels = { errors: {} } as unknown as EnrollmentLabels;
const noop = () => {};
const enroll = (over: Partial<EnrollPanelProps>): EnrollPanelProps => ({
  t, locale: 'pt-BR', labels, isEnrolled: false, currentStep: 1, isReadingMode: false, completedSteps: {},
  isRecording: false, liveVoiceLevel: 0, recordingElapsedSeconds: 0, captureError: null, jobBusy: false,
  feedback: null, onSelectStep: noop, onToggleReadingMode: noop, onStartStep: noop,
  onFinishStep: noop, onRedoStep: noop, onNextStep: noop, onBuildProfile: noop, onResetEnrollment: noop,
  ...over,
});
const modal = (over: Partial<AddSampleModalProps>): AddSampleModalProps => ({
  t, locale: 'pt-BR', labels, name: '', onNameChange: noop, captured: null, isRecording: false, liveVoiceLevel: 0,
  recordingElapsedSeconds: 0, captureError: null, jobBusy: false, feedback: null, playingAudioId: null,
  onStart: noop, onStop: noop, onPlayPreview: noop, onCancel: noop, onSave: noop, ...over,
});
const recordDisabled = (m: string) => /<button[^>]*class="record-btn-trigger"[^>]*disabled=""/.test(m);

describe('record buttons while the microphone is opening', () => {
  it('EnrollPanel disables record (and redo) while isStarting', () => {
    expect(recordDisabled(html(<EnrollPanel {...enroll({ isStarting: true })} />))).toBe(true);
    expect(recordDisabled(html(<EnrollPanel {...enroll({ isStarting: false })} />))).toBe(false);
    const withTake = html(<EnrollPanel {...enroll({ isStarting: true, completedSteps: { 1: { duration: 3 } } })} />);
    expect(withTake).toMatch(/<button[^>]*disabled=""[^>]*>voiceProfile.redoSample/);
  });
  it('AddSampleModal disables record while isStarting', () => {
    expect(recordDisabled(html(<AddSampleModal {...modal({ isStarting: true })} />))).toBe(true);
    expect(recordDisabled(html(<AddSampleModal {...modal({})} />))).toBe(false);
  });
});

describe('shouldReportUnstartedCapture', () => {
  it('reports busy and a cancellation nobody asked for; stays quiet for a requested cancel or a start', () => {
    expect(shouldReportUnstartedCapture('busy', false)).toBe(true);
    expect(shouldReportUnstartedCapture('cancelled', false)).toBe(true);
    expect(shouldReportUnstartedCapture('cancelled', true)).toBe(false);
    expect(shouldReportUnstartedCapture('started', false)).toBe(false);
    expect(shouldReportUnstartedCapture('failed', false)).toBe(false); // has its own physical-mic error
  });
});
