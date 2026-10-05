import { describe, it, expect } from 'vitest';
import { renderToStaticMarkup as html } from 'react-dom/server';
import type { EnrollmentLabels, ServiceSample } from '../enrollmentTypes';
import { profileIsStale, profileSampleIds } from '../hooks/voiceProfileLogic';
import { ProfilePanel, type ProfilePanelProps } from '../panels/ProfilePanel';
import { ptBR } from '../../i18n/locales/pt-BR';
import { enUS } from '../../i18n/locales/en-US';

const s = (id: string, over: Partial<ServiceSample> = {}): ServiceSample => ({
  id, name: id, timestamp: '', speechSeconds: 3, deviceLabel: 'M', usedInProfile: true, needsReenroll: false, otherMicrophone: false, ...over,
});

describe('profileIsStale', () => {
  it('is false without a build in this session and when the samples did not change', () => {
    expect(profileIsStale(['a', 'b'], null)).toBe(false);
    expect(profileIsStale(['b', 'a'], ['a', 'b'])).toBe(false);
  });
  it('is true after a sample was added or deleted since the build', () => {
    expect(profileIsStale(['a', 'b', 'c'], ['a', 'b'])).toBe(true);
    expect(profileIsStale(['a'], ['a', 'b'])).toBe(true);
    expect(profileIsStale([], ['a'])).toBe(true);
  });
  it('profileSampleIds keeps the current-microphone samples that have audio', () => {
    expect(profileSampleIds([s('a'), s('b', { otherMicrophone: true }), s('c', { needsReenroll: true }), s('d', { usedInProfile: false })])).toEqual(['a', 'd']);
  });
});

describe('ProfilePanel stale notice', () => {
  const t = (k: string) => k;
  const props = (stale: boolean): ProfilePanelProps => ({
    t, locale: 'pt-BR', labels: { errors: {} } as unknown as EnrollmentLabels,
    profileStatus: { is_enrolled: true, active_samples_count: 2, is_voice_profile_active: true },
    samplesCount: 2, canBuild: true, busy: false, feedback: null, onBuildProfile: () => {}, stale,
  });
  it('asks to rebuild only when stale, and never rebuilds by itself', () => {
    expect(html(<ProfilePanel {...props(true)} />)).toContain('voiceProfile.profileStale');
    expect(html(<ProfilePanel {...props(false)} />)).not.toContain('voiceProfile.profileStale');
    expect(ptBR.voiceProfile.profileStale).toMatch(/desatualizado/i);
    expect(enUS.voiceProfile.profileStale).toMatch(/out of date/i);
  });
});
