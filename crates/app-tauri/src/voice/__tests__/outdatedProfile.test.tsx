import { describe, it, expect } from 'vitest';
import { renderToStaticMarkup as html } from 'react-dom/server';
import type { EnrollmentLabels } from '../enrollmentTypes';
import type { VoiceProfileStatus } from '../../types';
import { ProfilePanel, type ProfilePanelProps } from '../panels/ProfilePanel';
import { profileStatusLabel } from '../panels/profileStatusLabel';
import {
  hasServiceVoiceProfile, isVoiceProfileCorrupted, isVoiceProfileOutdated,
  mergeVoiceProfileStatus, normalizeVoiceProfileStatus, voiceProfileActivationState,
} from '../hooks/voiceProfileLogic';
import { enrollmentErrorCode } from '../enrollmentErrors';
import { ptBR } from '../../i18n/locales/pt-BR';
import { enUS } from '../../i18n/locales/en-US';

const t = (path: string) => path;
const labels = { errors: {} } as unknown as EnrollmentLabels;
const noop = () => {};

// What GetStatus answers when a v1 (FiLM) profile is on disk (crates/service, profile schema v2).
const SERVICE_OUTDATED = {
  stored_voice_profile_id: null,
  has_voice_profile: false,
  is_voice_profile_active: false,
  active_voice_profile_id: null,
  voice_profile_error: 'ENROLL_PROFILE_OUTDATED',
  voice_profile_state: 'outdated',
  voice_profile_outdated: true,
  voice_profile_can_regenerate: true,
  voice_profile_model_id: 'pdfnet3-pro-v3-full',
  voice_profile_model_label: 'pDFNet3 Pro v3',
  voice_samples_count: 5,
};

const props = (over: Partial<ProfilePanelProps> = {}): ProfilePanelProps => ({
  t, labels, locale: 'pt-BR', samplesCount: 5, canBuild: true, busy: false, feedback: null,
  onBuildProfile: noop, onResetEnrollment: noop,
  profileStatus: normalizeVoiceProfileStatus(SERVICE_OUTDATED), ...over,
});

describe('outdated (v1) voice profile', () => {
  it('is recognised from any of the three service signals and is never "corrupted" nor "a profile"', () => {
    const status = normalizeVoiceProfileStatus(SERVICE_OUTDATED);
    expect(isVoiceProfileOutdated(status)).toBe(true);
    expect(isVoiceProfileOutdated({ voice_profile_state: 'outdated' })).toBe(true);
    expect(isVoiceProfileOutdated({ voice_profile_error: 'ENROLL_PROFILE_OUTDATED' })).toBe(true);
    expect(isVoiceProfileOutdated({ voice_profile_state: 'active' })).toBe(false);
    expect(isVoiceProfileCorrupted('ENROLL_PROFILE_OUTDATED')).toBe(false);
    expect(hasServiceVoiceProfile(status)).toBe(false);
    expect(voiceProfileActivationState(status)).toBe('outdated');
    expect(status.is_enrolled).toBe(false);
  });

  it('keeps the new service fields through normalize and merge, and drops free text', () => {
    const merged = mergeVoiceProfileStatus({ is_enrolled: false, active_samples_count: 0 }, SERVICE_OUTDATED, 5);
    expect(merged.voice_profile_state).toBe('outdated');
    expect(merged.voice_profile_can_regenerate).toBe(true);
    expect(merged.voice_profile_model_label).toBe('pDFNet3 Pro v3');
    const hostile = normalizeVoiceProfileStatus({ ...SERVICE_OUTDATED, voice_profile_model_label: '<b>x</b>', voice_profile_state: 'weird' });
    expect(hostile.voice_profile_model_label).toBeUndefined();
    expect(hostile.voice_profile_state).toBeUndefined();
  });

  it('status label is "outdated" (orange), not corrupted and not active', () => {
    const label = profileStatusLabel(normalizeVoiceProfileStatus(SERVICE_OUTDATED), t);
    expect(label).toEqual({ text: 'voiceProfile.statusOutdated', active: false, corrupted: false });
  });

  it('the panel asks to regenerate from the saved samples and shows the model', () => {
    const m = html(<ProfilePanel {...props()} />);
    expect(m).toContain('profile-outdated-banner');
    expect(m).toContain('voiceProfile.outdatedTitle');
    expect(m).toContain('voiceProfile.outdatedDesc');
    expect(m).toContain('regenerate-profile-btn');
    expect(m).toContain('voiceProfile.regenerateProfileBtn');
    expect(m).toContain('pDFNet3 Pro v3');
    expect(m).not.toContain('profile-corrupted-banner');
    expect(m).not.toContain('profile-empty-banner');
    expect(m).not.toContain('profile-active-banner');
  });

  it('regenerate is disabled while busy or building', () => {
    expect(html(<ProfilePanel {...props({ busy: true })} />)).toMatch(/<button[^>]*regenerate-profile-btn[^>]*disabled=""|<button[^>]*disabled=""[^>]*regenerate-profile-btn/);
    expect(html(<ProfilePanel {...props({ isBuilding: true })} />)).toContain('voiceProfile.buildingProfile');
  });

  it('without enough saved samples it asks to record again instead of offering regeneration', () => {
    const noSamples = normalizeVoiceProfileStatus({ ...SERVICE_OUTDATED, voice_profile_can_regenerate: false });
    const m = html(<ProfilePanel {...props({ profileStatus: noSamples })} />);
    expect(m).toContain('voiceProfile.outdatedNoSamplesDesc');
    expect(m).not.toContain('regenerate-profile-btn');
    expect(m).toContain('voiceProfile.startEnrollBtn');
  });

  it('the error code is known to the enrollment client', () => {
    expect(enrollmentErrorCode({ errorCode: 'ENROLL_PROFILE_OUTDATED' })).toBe('ENROLL_PROFILE_OUTDATED');
  });

  it('pt-BR and en-US carry every new string', () => {
    for (const locale of [ptBR, enUS]) {
      const vp = (locale as unknown as { voiceProfile: Record<string, unknown> }).voiceProfile;
      for (const key of ['statusOutdated', 'outdatedTitle', 'outdatedDesc', 'outdatedNoSamplesDesc', 'regenerateProfileBtn', 'regenerateProfileHint', 'modelTitle']) {
        expect(typeof vp[key], key).toBe('string');
        expect((vp[key] as string).length).toBeGreaterThan(3);
      }
      const errors = (vp.enrollment as { errors: Record<string, string> }).errors;
      expect(errors.ENROLL_PROFILE_OUTDATED.length).toBeGreaterThan(10);
    }
    expect((ptBR as unknown as { voiceProfile: { regenerateProfileBtn: string } }).voiceProfile.regenerateProfileBtn).toContain('Regenerar perfil');
    expect((enUS as unknown as { voiceProfile: { regenerateProfileBtn: string } }).voiceProfile.regenerateProfileBtn).toContain('Regenerate profile');
  });
});

describe('the profile model is reported by the service only', () => {
  it('no model label without the service field', () => {
    const status: VoiceProfileStatus = { is_enrolled: true, active_samples_count: 1, stored_voice_profile_id: 'p1', is_voice_profile_active: true };
    expect(html(<ProfilePanel {...props({ profileStatus: status })} />)).not.toContain('profile-model-label');
    const withModel = normalizeVoiceProfileStatus({ stored_voice_profile_id: 'p1', is_voice_profile_active: true, voice_profile_model_label: 'pDFNet3 Pro v3' });
    expect(html(<ProfilePanel {...props({ profileStatus: withModel })} />)).toContain('profile-model-label');
  });
});
