import { describe, it, expect } from 'vitest';
import { renderToStaticMarkup as html } from 'react-dom/server';
import { ptBR } from '../../i18n/locales/pt-BR';
import { enUS } from '../../i18n/locales/en-US';
import type { EnrollmentLabels, ServiceSample } from '../enrollmentTypes';
import { errorLabelForJob, samplesUsedInProfile } from '../hooks/voiceProfileLogic';
import { EnrollPanel, type EnrollPanelProps } from '../panels/EnrollPanel';

const t = (path: string) => `T:${path}`;
const labels = { errors: { ENROLL_TOO_LITTLE_SPEECH: 'short-sample', ENROLL_TOO_QUIET: 'quiet', UNKNOWN: 'unknown' } } as unknown as EnrollmentLabels;

describe('errorLabelForJob', () => {
  it('a profile build with too little speech asks to record more with this microphone', () => {
    expect(errorLabelForJob('ENROLL_TOO_LITTLE_SPEECH', 'build', labels, t)).toBe('T:voiceProfile.buildTooLittleSpeech');
  });
  it('a short sample keeps the sample label; other codes keep theirs', () => {
    expect(errorLabelForJob('ENROLL_TOO_LITTLE_SPEECH', 'sample', labels, t)).toBe('short-sample');
    expect(errorLabelForJob('ENROLL_TOO_QUIET', 'build', labels, t)).toBe('quiet');
    expect(errorLabelForJob(null, 'build', labels, t)).toBe('unknown');
  });
  it('the build label is distinct and says to record more with this microphone', () => {
    expect(ptBR.voiceProfile.buildTooLittleSpeech).toContain('este microfone');
    expect(ptBR.voiceProfile.buildTooLittleSpeech).not.toBe(ptBR.voiceProfile.enrollment.errors.ENROLL_TOO_LITTLE_SPEECH);
    expect(enUS.voiceProfile.buildTooLittleSpeech).toContain('this microphone');
  });
});

describe('samplesUsedInProfile', () => {
  const s = (id: string, over: Partial<ServiceSample>): ServiceSample => ({
    id, name: id, timestamp: '', speechSeconds: 3, deviceLabel: 'M', usedInProfile: false, needsReenroll: false, otherMicrophone: false, ...over,
  });
  it('counts only samples used in the profile (not other-microphone nor re-record)', () => {
    expect(samplesUsedInProfile([s('a', { usedInProfile: true }), s('b', { otherMicrophone: true }), s('c', { needsReenroll: true }), s('d', { usedInProfile: true })])).toBe(2);
    expect(samplesUsedInProfile([])).toBe(0);
  });
});

describe('re-enrollment hint', () => {
  it('explains that old samples stay in the service and use the budget until deleted in the Gallery', () => {
    const noop = () => {};
    const props = {
      t, locale: 'pt-BR', labels, isEnrolled: true, currentStep: 1, isReadingMode: false, completedSteps: {},
      isRecording: false, liveVoiceLevel: 0, recordingElapsedSeconds: 0, captureError: null, jobBusy: false,
      playingAudioId: null, feedback: null, onSelectStep: noop, onToggleReadingMode: noop, onStartStep: noop,
      onFinishStep: noop, onRedoStep: noop, onNextStep: noop, onPlayStep: noop, onBuildProfile: noop, onResetEnrollment: noop,
    } as EnrollPanelProps;
    expect(html(<EnrollPanel {...props} />)).toContain('T:voiceProfile.reEnrollHint');
    expect(ptBR.voiceProfile.reEnrollHint).toMatch(/Galeria/);
    expect(ptBR.voiceProfile.reEnrollHint).toMatch(/orçamento/);
    expect(enUS.voiceProfile.reEnrollHint).toMatch(/Gallery/);
  });
});
