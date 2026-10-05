import { describe, it, expect, vi } from 'vitest';
import {
  VOICE_TAB_IDS,
  VOICE_TAB_STORAGE_KEY,
  parseVoiceTab,
  loadVoiceTab,
  saveVoiceTab,
  tabAfterError,
  voiceTabBadges,
  jobFeedbackFor,
  resolveTabChange,
} from '../panels/voiceTabs';

describe('persisted voice tab', () => {
  it('uses the agreed storage key and tab order', () => {
    expect(VOICE_TAB_STORAGE_KEY).toBe('clearcore_voice_active_tab');
    expect(VOICE_TAB_IDS).toEqual(['enroll', 'gallery', 'calls', 'profile']);
  });
  it('accepts only known ids; anything else falls back to the first tab', () => {
    for (const id of VOICE_TAB_IDS) expect(parseVoiceTab(id)).toBe(id);
    for (const bad of [null, undefined, '', 'samples', 'intake', 'GALLERY', ' gallery', 42, {}, '__proto__', 'toString']) {
      expect(parseVoiceTab(bad), String(bad)).toBe('enroll');
    }
  });
  it('loads from storage and survives a missing or throwing storage', () => {
    expect(loadVoiceTab({ getItem: () => 'calls' })).toBe('calls');
    expect(loadVoiceTab({ getItem: () => 'nope' })).toBe('enroll');
    expect(loadVoiceTab({ getItem: () => { throw new Error('blocked'); } })).toBe('enroll');
    expect(loadVoiceTab(undefined)).toBe('enroll');
  });
  it('saves under the key and swallows storage errors', () => {
    const setItem = vi.fn();
    saveVoiceTab('profile', { setItem });
    expect(setItem).toHaveBeenCalledWith('clearcore_voice_active_tab', 'profile');
    expect(() => saveVoiceTab('profile', { setItem: () => { throw new Error('quota'); } })).not.toThrow();
    expect(() => saveVoiceTab('profile', undefined)).not.toThrow();
  });
});

describe('tabAfterError', () => {
  it('switches to the gallery on the budget error, from any tab', () => {
    for (const id of VOICE_TAB_IDS) expect(tabAfterError(id, 'ENROLL_BUDGET_EXCEEDED')).toBe('gallery');
  });
  it('stays on the current tab for any other error or none', () => {
    expect(tabAfterError('enroll', 'ENROLL_TOO_QUIET')).toBe('enroll');
    expect(tabAfterError('calls', 'SERVICE_UNAVAILABLE')).toBe('calls');
    expect(tabAfterError('profile', null)).toBe('profile');
  });
});

describe('voiceTabBadges', () => {
  it('gallery shows the sample count, calls only the pending takes when there are any', () => {
    expect(voiceTabBadges(3, 2)).toEqual({ gallery: 3, calls: 2 });
    expect(voiceTabBadges(0, 0)).toEqual({ gallery: 0, calls: undefined });
    expect(voiceTabBadges(5, 0)).toEqual({ gallery: 5, calls: undefined });
  });
});

describe('jobFeedbackFor', () => {
  const view = { jobBusy: false, currentJob: null, enrollErrorText: 'x' };
  it('a profile build started on Enrollment shows on Enrollment and on Profile only', () => {
    const src = { origin: 'enroll', kind: 'build' } as const;
    expect(jobFeedbackFor('enroll', src, view)).toBe(view);
    expect(jobFeedbackFor('profile', src, view)).toBe(view);
    expect(jobFeedbackFor('gallery', src, view)).toBeNull();
    expect(jobFeedbackFor('calls', src, view)).toBeNull();
  });
  it('a profile build started on Profile shows on Profile only', () => {
    const src = { origin: 'profile', kind: 'build' } as const;
    expect(jobFeedbackFor('profile', src, view)).toBe(view);
    expect(jobFeedbackFor('enroll', src, view)).toBeNull();
  });
  it('a sample job shows only where it started; nothing without a job', () => {
    expect(jobFeedbackFor('enroll', { origin: 'enroll', kind: 'sample' }, view)).toBe(view);
    expect(jobFeedbackFor('profile', { origin: 'enroll', kind: 'sample' }, view)).toBeNull();
    expect(jobFeedbackFor('gallery', { origin: 'gallery', kind: 'sample' }, view)).toBe(view);
    for (const tab of VOICE_TAB_IDS) expect(jobFeedbackFor(tab, null, view)).toBeNull();
  });
});

describe('resolveTabChange', () => {
  it('a user choice is validated and persisted', () => {
    expect(resolveTabChange('enroll', { by: 'user', id: 'calls' })).toEqual({ tab: 'calls', persist: true });
    expect(resolveTabChange('calls', { by: 'user', id: 'bogus' })).toEqual({ tab: 'enroll', persist: true });
  });
  it('the automatic gallery switch on the budget error is not persisted', () => {
    for (const id of VOICE_TAB_IDS) expect(resolveTabChange(id, { by: 'budget-error' })).toEqual({ tab: 'gallery', persist: false });
  });
});

describe('tab labels', () => {
  it('exist in both languages with the agreed names, honest wording', async () => {
    const { ptBR } = await import('../../i18n/locales/pt-BR');
    const { enUS } = await import('../../i18n/locales/en-US');
    const pick = (v: typeof ptBR.voiceProfile) => [v.tabEnroll, v.tabGallery, v.tabCalls, v.tabProfile, v.tabsAriaLabel];
    expect(pick(ptBR.voiceProfile).slice(0, 4)).toEqual(['Cadastro', 'Galeria', 'Chamadas', 'Perfil']);
    expect(pick(enUS.voiceProfile).slice(0, 4)).toEqual(['Enrollment', 'Gallery', 'Calls', 'Profile']);
    for (const text of [...pick(ptBR.voiceProfile), ...pick(enUS.voiceProfile)]) {
      expect(text.length).toBeGreaterThan(0);
      expect(text).not.toMatch(/end-to-end|produ[cç][aã]o|production/i);
    }
  });
  it('the labels of the old two-tab layout are gone', async () => {
    const { ptBR } = await import('../../i18n/locales/pt-BR');
    const { enUS } = await import('../../i18n/locales/en-US');
    for (const v of [ptBR.voiceProfile, enUS.voiceProfile]) {
      expect('tabSamples' in v).toBe(false);
      expect('tabCallSuggestions' in v).toBe(false);
    }
  });
});
