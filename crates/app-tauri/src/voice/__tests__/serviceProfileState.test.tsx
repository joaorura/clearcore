import { describe, it, expect } from 'vitest';
import { renderToStaticMarkup as html } from 'react-dom/server';
import type { VoiceProfileStatus } from '../../types';
import type { EnrollmentLabels } from '../enrollmentTypes';
import {
  applySetVoiceProfileResult,
  hasServiceVoiceProfile,
  mergeVoiceProfileStatus,
  normalizeVoiceProfileStatus,
  showStaleProfileNotice,
  voiceProfileActivationState,
  voiceProfileStatusLabelKey,
} from '../hooks/voiceProfileLogic';
import { ProfilePanel, type ProfilePanelProps } from '../panels/ProfilePanel';
import { EnrollPanel, type EnrollPanelProps } from '../panels/EnrollPanel';

const t = (k: string) => k;
const labels = { errors: {} } as unknown as EnrollmentLabels;
const noop = () => {};

/** As GetStatus (merged by the main process) reports a profile the service holds. */
const serviceStored: VoiceProfileStatus = normalizeVoiceProfileStatus({
  is_enrolled: false, active_samples_count: 2, stored_voice_profile_id: 'p1', has_voice_profile: true, is_voice_profile_active: false,
});
const serviceActive: VoiceProfileStatus = normalizeVoiceProfileStatus({
  active_samples_count: 2, stored_voice_profile_id: 'p1', has_voice_profile: true, is_voice_profile_active: true,
});
/** Old flow: the local file says enrolled, the service holds no profile. */
const legacyLocalOnly: VoiceProfileStatus = normalizeVoiceProfileStatus({
  is_enrolled: true, active_samples_count: 3, stored_voice_profile_id: null, has_voice_profile: false, is_voice_profile_active: false,
});

describe('profile existence comes from the service (I1)', () => {
  it('derives is_enrolled from stored_voice_profile_id / has_voice_profile, ignoring the local flag', () => {
    expect(serviceStored.is_enrolled).toBe(true);
    expect(hasServiceVoiceProfile(serviceStored)).toBe(true);
    expect(normalizeVoiceProfileStatus({ has_voice_profile: true }).is_enrolled).toBe(true);
    expect(normalizeVoiceProfileStatus({ is_enrolled: true }).is_enrolled).toBe(false);
    expect(normalizeVoiceProfileStatus({ is_enrolled: true, stored_voice_profile_id: '' }).is_enrolled).toBe(false);
    expect(hasServiceVoiceProfile({ is_enrolled: true, active_samples_count: 1 })).toBe(false);
  });

  it('legacy local is_enrolled:true without a service profile is not "stored, not applied"', () => {
    expect(legacyLocalOnly.is_enrolled).toBe(false);
    expect(voiceProfileActivationState(legacyLocalOnly)).toBe('none');
    expect(voiceProfileStatusLabelKey(legacyLocalOnly)).toBe('none');
    const m = html(<ProfilePanel {...profileProps({ profileStatus: legacyLocalOnly })} />);
    expect(m).not.toContain('voiceProfile.storedNotApplied');
    expect(m).toContain('voiceProfile.activateProfileBtn');
    expect(m).not.toContain('voiceProfile.rebuildProfileBtn');
  });

  it('a profile the service holds shows "stored, not applied" or active', () => {
    expect(voiceProfileStatusLabelKey(serviceStored)).toBe('storedNotApplied');
    expect(voiceProfileStatusLabelKey(serviceActive)).toBe('active');
  });

  it('merge and set results take the flag from the service answer', () => {
    const initial: VoiceProfileStatus = { is_enrolled: false, active_samples_count: 0 };
    expect(mergeVoiceProfileStatus(initial, { profile: { is_enrolled: false, stored_voice_profile_id: 'p1' } }, 0).is_enrolled).toBe(true);
    expect(mergeVoiceProfileStatus(initial, { profile: { is_enrolled: true, stored_voice_profile_id: null } }, 0).is_enrolled).toBe(false);
    const prev = { ...serviceActive };
    expect(applySetVoiceProfileResult(prev, prev, { is_enrolled: true, stored_voice_profile_id: null }).is_enrolled).toBe(false);
    expect(applySetVoiceProfileResult(prev, prev, undefined).is_enrolled).toBe(false);
    expect(applySetVoiceProfileResult(legacyLocalOnly, legacyLocalOnly, { stored_voice_profile_id: 'p2' }).is_enrolled).toBe(true);
  });

  it('the stale notice needs a service profile and changed samples', () => {
    expect(showStaleProfileNotice(serviceActive, ['a', 'b'], ['a'])).toBe(true);
    expect(showStaleProfileNotice(serviceActive, ['a'], ['a'])).toBe(false);
    expect(showStaleProfileNotice(legacyLocalOnly, ['a', 'b'], ['a'])).toBe(false);
    expect(showStaleProfileNotice(serviceActive, null, ['a'])).toBe(false);
  });
});

const profileProps = (over: Partial<ProfilePanelProps> = {}): ProfilePanelProps => ({
  t, labels, locale: 'pt-BR', profileStatus: { is_enrolled: false, active_samples_count: 0 }, samplesCount: 2,
  canBuild: true, busy: false, feedback: null, onBuildProfile: noop, ...over,
});

describe('profile actions come back with a service profile (I1)', () => {
  it('ProfilePanel offers "Refazer perfil" for a service profile', () => {
    expect(html(<ProfilePanel {...profileProps({ profileStatus: serviceStored })} />)).toContain('voiceProfile.rebuildProfileBtn');
  });

  const enrollProps = (isEnrolled: boolean): EnrollPanelProps => ({
    t, locale: 'pt-BR', labels, isEnrolled, currentStep: 1, isReadingMode: false, completedSteps: {},
    isRecording: false, liveVoiceLevel: 0, recordingElapsedSeconds: 0, captureError: null, jobBusy: false,
    playingAudioId: null, feedback: null, onSelectStep: noop, onToggleReadingMode: noop, onStartStep: noop,
    onFinishStep: noop, onRedoStep: noop, onNextStep: noop, onPlayStep: noop, onBuildProfile: noop, onResetEnrollment: noop,
  });
  it('EnrollPanel offers "Refazer Cadastro Completo" when the service holds a profile', () => {
    expect(html(<EnrollPanel {...enrollProps(hasServiceVoiceProfile(serviceStored))} />)).toContain('voiceProfile.reEnrollBtn');
    expect(html(<EnrollPanel {...enrollProps(hasServiceVoiceProfile(legacyLocalOnly))} />)).not.toContain('voiceProfile.reEnrollBtn');
  });
});
