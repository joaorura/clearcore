import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { invokeBridge } from '../bridge';
import { normalizeVoiceProfileStatus, mergeVoiceProfileStatus, voiceProfileActivationState, voiceProfileStatusLabelKey, stripServiceVoiceProfileKeys, applySetVoiceProfileResult, voiceProfileErrorKey } from '../VoiceProfileCard';
import { ptBR } from '../i18n/locales/pt-BR';
import { enUS } from '../i18n/locales/en-US';
import type { VoiceProfileStatus, CallSuggestionTake, StudioPreset } from '../types';
import fs from 'fs';
import path from 'path';
import os from 'os';

// Test the Node.js VoiceProfileStore module directly
// eslint-disable-next-line @typescript-eslint/no-require-imports
const voiceProfileStore = require('../../electron/voice-profile-store.cjs');

describe('VoiceProfileStore (electron/voice-profile-store.cjs)', () => {
  const testDir = path.join(os.tmpdir(), `clearcore-test-store-${Date.now()}`);

  beforeEach(() => {
    process.env.CLEARCORE_CONFIG_DIR = testDir;
    if (fs.existsSync(testDir)) {
      fs.rmSync(testDir, { recursive: true, force: true });
    }
  });

  afterEach(() => {
    delete process.env.CLEARCORE_CONFIG_DIR;
    if (fs.existsSync(testDir)) {
      fs.rmSync(testDir, { recursive: true, force: true });
    }
  });

  it('reads default voice profile if no file exists', () => {
    const profile = voiceProfileStore.readVoiceProfile(testDir);
    expect(profile.is_enrolled).toBe(false);
    expect(profile.active_samples_count).toBe(0);
    // Service-only fields are never invented by the local store.
    expect(profile.embedding_dim).toBeUndefined();
    expect(profile.gain_boost_db).toBeUndefined();
    expect(profile.neural_eq_calibrated).toBeUndefined();
  });

  it('writes and persists voice profile correctly', () => {
    const updated = voiceProfileStore.writeVoiceProfile(
      {
        is_enrolled: true,
        active_samples_count: 5,
        neural_eq_calibrated: true,
      },
      testDir
    );

    expect(updated.is_enrolled).toBe(true);
    expect(updated.active_samples_count).toBe(5);

    const reRead = voiceProfileStore.readVoiceProfile(testDir);
    expect(reRead.is_enrolled).toBe(true);
    expect(reRead.active_samples_count).toBe(5);
  });

  it('keeps no local sample store (samples live in the service)', () => {
    for (const gone of ['addVoiceSample', 'deleteVoiceSample', 'writeVoiceSamples']) {
      expect(voiceProfileStore[gone], gone).toBeUndefined();
    }
    expect(voiceProfileStore.readVoiceSamples).toBeUndefined();
  });

  it('handles call suggestion intake (approve & dismiss)', () => {
    const take1 = {
      id: 'take-meet-1',
      title: 'Chamada Google Meet',
      timestamp: '11:15',
      durationSec: 6.2,
      snrDb: 24.5,
    };

    voiceProfileStore.writeCallTakes([take1], testDir);
    expect(voiceProfileStore.readCallTakes(testDir)).toHaveLength(1);

    // Approve take
    // Only drops the take from the local cache; the sample is created by the service.
    const approveRes = voiceProfileStore.approveCallTake('take-meet-1', testDir);
    expect(approveRes.success).toBe(true);
    expect(approveRes.takes).toHaveLength(0);
    expect(approveRes.sample).toBeUndefined();

    // Dismiss take test
    voiceProfileStore.writeCallTakes([take1], testDir);
    const dismissRes = voiceProfileStore.dismissCallTake('take-meet-1', testDir);
    expect(dismissRes.success).toBe(true);
    expect(dismissRes.takes).toHaveLength(0);
  });
});

describe('IPC Bridge invokeBridge Integration for Voice Profile & Studio DSP', () => {
  let originalWindow: unknown;
  let originalLocalStorage: unknown;
  const mockStorage: Record<string, string> = {};

  const fakeLocalStorage = {
    getItem: (key: string) => mockStorage[key] ?? null,
    setItem: (key: string, val: string) => {
      mockStorage[key] = String(val);
    },
    removeItem: (key: string) => {
      delete mockStorage[key];
    },
    clear: () => {
      for (const k of Object.keys(mockStorage)) delete mockStorage[k];
    },
  };

  beforeEach(() => {
    originalWindow = (globalThis as unknown as { window?: unknown }).window;
    originalLocalStorage = (globalThis as unknown as { localStorage?: unknown }).localStorage;
    (globalThis as unknown as { localStorage: unknown }).localStorage = fakeLocalStorage;
    fakeLocalStorage.clear();
  });

  afterEach(() => {
    fakeLocalStorage.clear();
    (globalThis as unknown as { window: unknown }).window = originalWindow;
    (globalThis as unknown as { localStorage: unknown }).localStorage = originalLocalStorage;
    vi.restoreAllMocks();
  });

  it('invokes window.clearcoreApi when available in Electron environment', async () => {
    const fakeWindow: Record<string, unknown> = {};
    (globalThis as unknown as { window: unknown }).window = fakeWindow;
    const mockSetProfile = vi.fn().mockResolvedValue({ success: true });
    const mockGetProfile = vi.fn().mockResolvedValue({
      is_enrolled: true,
      active_samples_count: 5,
      embedding_dim: 192,
      neural_eq_calibrated: true,
      gain_boost_db: 1.8,
    });
    const mockGetTakes = vi.fn().mockResolvedValue([{ id: 't1', title: 'Take 1' }]);
    const mockApproveTake = vi.fn().mockResolvedValue({ success: true, id: 't1' });
    const mockDismissTake = vi.fn().mockResolvedValue({ success: true, id: 't1' });
    const mockGetPreset = vi.fn().mockResolvedValue('Natural');
    const mockSetPreset = vi.fn().mockResolvedValue({ success: true, preset: 'Podcast' });

    fakeWindow.clearcoreApi = {
      getStatus: vi.fn(),
      setMode: vi.fn(),
      restartGeneration: vi.fn(),
      getDiagnostics: vi.fn(),
      getAutostart: vi.fn(),
      setAutostart: vi.fn(),
      minimizeToTray: vi.fn(),
      quitApp: vi.fn(),
      getVirtualMicStatus: vi.fn(),
      recreateVirtualMic: vi.fn(),
      setDefaultVirtualMic: vi.fn(),
      getInputDevices: vi.fn(),
      setVoiceProfile: mockSetProfile,
      getVoiceProfile: mockGetProfile,
      getCallTakes: mockGetTakes,
      approveCallTake: mockApproveTake,
      dismissCallTake: mockDismissTake,
      getStudioPreset: mockGetPreset,
      setStudioPreset: mockSetPreset,
      onStatusUpdate: vi.fn(),
      onVirtualMicUpdate: vi.fn(),
      onInputDevicesUpdate: vi.fn(),
      onAutostartUpdate: vi.fn(),
    };

    // 1. set_voice_profile
    const setRes = await invokeBridge<{ success: boolean }>('set_voice_profile', {
      profile: { is_enrolled: true, active_samples_count: 5 },
    });
    expect(setRes.success).toBe(true);
    expect(mockSetProfile).toHaveBeenCalled();

    // 2. get_voice_profile
    const getRes = await invokeBridge<VoiceProfileStatus>('get_voice_profile');
    expect(getRes.is_enrolled).toBe(true);
    expect(mockGetProfile).toHaveBeenCalled();

    // 6. get_call_takes
    const takesRes = await invokeBridge<CallSuggestionTake[]>('get_call_takes');
    expect(takesRes).toHaveLength(1);
    expect(mockGetTakes).toHaveBeenCalled();

    // 7. approve_call_take
    await invokeBridge('approve_call_take', { id: 't1', name: 'Take 1' });
    expect(mockApproveTake).toHaveBeenCalledWith('t1', 'Take 1', undefined);

    // 8. dismiss_call_take
    await invokeBridge('dismiss_call_take', { id: 't1' });
    expect(mockDismissTake).toHaveBeenCalledWith('t1');

    // 9. get_studio_preset
    const presetRes = await invokeBridge<StudioPreset>('get_studio_preset');
    expect(presetRes).toBe('Natural');
    expect(mockGetPreset).toHaveBeenCalled();

    // 10. set_studio_preset
    const setPresetRes = await invokeBridge<{ success: boolean; preset: string }>('set_studio_preset', { preset: 'Podcast' });
    expect(setPresetRes.preset).toBe('Podcast');
    expect(mockSetPreset).toHaveBeenCalledWith('Podcast');
  });

  it('gracefully falls back in browser environment without throwing errors', async () => {
    // In browser fallback (no clearcoreApi, no __TAURI_INTERNALS__)
    const profileRes = await invokeBridge<VoiceProfileStatus>('get_voice_profile');
    expect(profileRes.embedding_dim).toBeUndefined();
    expect(profileRes.gain_boost_db).toBeUndefined();

    const setRes = await invokeBridge<{ success: boolean }>('set_voice_profile', {
      profile: { is_enrolled: true, active_samples_count: 5 },
    });
    expect(setRes.success).toBe(true);

    // A non-boolean is_enrolled never defaults to enrolled.
    const unset = await invokeBridge<VoiceProfileStatus>('set_voice_profile', { profile: {} });
    expect(unset.is_enrolled).toBe(false);

    // No local voice state (M-6): nothing is written to the legacy keys, nothing is read back.
    for (const k of ['clearcore_voice_profile_enrolled', 'clearcore_voice_profile_samples', 'clearcore_voice_intake_takes', 'clearcore_voice_sample_count']) {
      expect(fakeLocalStorage.getItem(k), k).toBeNull();
    }
    fakeLocalStorage.setItem('clearcore_voice_profile_enrolled', 'true');
    fakeLocalStorage.setItem('clearcore_voice_intake_takes', JSON.stringify([{ id: 'old' }]));
    expect((await invokeBridge<VoiceProfileStatus>('get_voice_profile')).is_enrolled).toBe(false);
    expect(await invokeBridge<unknown[]>('get_call_takes')).toEqual([]);

    const presetRes = await invokeBridge<StudioPreset>('get_studio_preset');
    expect(presetRes).toBe('Natural');

    const setPresetRes = await invokeBridge<{ success: boolean; preset: string }>('set_studio_preset', {
      preset: 'Broadcast',
    });
    expect(setPresetRes.success).toBe(true);
    expect(setPresetRes.preset).toBe('Broadcast');
  });
});

describe('normalizeVoiceProfileStatus', () => {
  const base = { is_enrolled: true, active_samples_count: 3, embedding_dim: 192, neural_eq_calibrated: false };
  it('reads a flat status', () => {
    expect(normalizeVoiceProfileStatus(base).active_samples_count).toBe(3);
  });
  it('reads a nested-only envelope', () => {
    expect(normalizeVoiceProfileStatus({ success: true, profile: base }).active_samples_count).toBe(3);
  });
  it('defaults safely for an empty envelope', () => {
    const s = normalizeVoiceProfileStatus({ success: false });
    expect(s.is_enrolled).toBe(false);
    expect(s.active_samples_count).toBe(0);
  });
});

describe('normalizeVoiceProfileStatus edge cases', () => {
  it('handles null and undefined', () => {
    expect(normalizeVoiceProfileStatus(null).active_samples_count).toBe(0);
    expect(normalizeVoiceProfileStatus(undefined).is_enrolled).toBe(false);
  });
  it('prefers nested profile over flat fields', () => {
    const s = normalizeVoiceProfileStatus({ is_enrolled: false, active_samples_count: 1, profile: { is_enrolled: true, active_samples_count: 5 } });
    expect(s.is_enrolled).toBe(true);
    expect(s.active_samples_count).toBe(5);
  });
});

describe('mergeVoiceProfileStatus', () => {
  const initial: VoiceProfileStatus = { is_enrolled: true, active_samples_count: 2, embedding_dim: 192, neural_eq_calibrated: true, gain_boost_db: 1.8 };
  it('keeps initial for failed or null responses', () => {
    expect(mergeVoiceProfileStatus(initial, { success: false }, 2)).toEqual(initial);
    expect(mergeVoiceProfileStatus(initial, null, 2)).toEqual(initial);
    expect(mergeVoiceProfileStatus(initial, undefined, 2)).toEqual(initial);
  });
  it('does not overwrite with defaults on a partial response', () => {
    const m = mergeVoiceProfileStatus(initial, { profile: { is_enrolled: true, active_samples_count: 9 } }, 0);
    expect(m.embedding_dim).toBe(192);
    expect(m.gain_boost_db).toBe(1.8);
    expect(m.active_samples_count).toBe(9);
  });
  it('prefers local sample count when present', () => {
    expect(mergeVoiceProfileStatus(initial, { is_enrolled: false, active_samples_count: 9 }, 2).active_samples_count).toBe(2);
  });
});

describe('voiceProfileActivationState', () => {
  const base: VoiceProfileStatus = {
    is_enrolled: true,
    active_samples_count: 3,
    embedding_dim: 192,
    neural_eq_calibrated: true,
  };

  it('returns active only when the service confirms it', () => {
    expect(voiceProfileActivationState({ ...base, is_voice_profile_active: true })).toBe('active');
  });

  it('returns stored_not_applied for a stored but inactive profile, never active', () => {
    const s = { ...base, is_voice_profile_active: false, stored_voice_profile_id: 'abc' };
    expect(voiceProfileActivationState(s)).toBe('stored_not_applied');
    expect(voiceProfileActivationState({ ...base, is_voice_profile_active: false })).toBe('stored_not_applied');
    expect(voiceProfileActivationState({ ...base, is_enrolled: false, stored_voice_profile_id: 'abc' })).toBe('stored_not_applied');
  });

  it('does not claim active when the field is absent (older service)', () => {
    expect(voiceProfileActivationState(base)).not.toBe('active');
    expect(voiceProfileActivationState({ ...base, is_enrolled: false })).toBe('none');
  });

  it('passes the new fields through normalize and merge without inventing defaults', () => {
    const n = normalizeVoiceProfileStatus({
      ...base,
      is_voice_profile_active: false,
      stored_voice_profile_id: 'abc',
      voice_profile_error: 'model missing',
    });
    expect(n.is_voice_profile_active).toBe(false);
    expect(n.stored_voice_profile_id).toBe('abc');
    expect(n.voice_profile_error).toBe('model missing');
    const absent = normalizeVoiceProfileStatus(base);
    expect(absent.is_voice_profile_active).toBeUndefined();
    expect(absent.stored_voice_profile_id).toBeUndefined();
    expect(absent.voice_profile_error).toBeUndefined();
    const m = mergeVoiceProfileStatus(base, { ...base, is_voice_profile_active: false, voice_profile_error: 'x' }, 3);
    expect(m.is_voice_profile_active).toBe(false);
    expect(m.voice_profile_error).toBe('x');
  });
});

describe('voiceProfileStatusLabelKey', () => {
  const base: VoiceProfileStatus = {
    is_enrolled: true,
    active_samples_count: 3,
    embedding_dim: 192,
    neural_eq_calibrated: true,
  };

  it('maps each state to its label key', () => {
    expect(voiceProfileStatusLabelKey({ ...base, is_voice_profile_active: true })).toBe('active');
    expect(voiceProfileStatusLabelKey({ ...base, is_voice_profile_active: false, stored_voice_profile_id: 'abc' })).toBe('storedNotApplied');
    expect(voiceProfileStatusLabelKey({ ...base, is_enrolled: false })).toBe('none');
  });

  it('never returns active for enrolled without confirmation', () => {
    expect(voiceProfileStatusLabelKey(base)).toBe('enrolledUnconfirmed');
    expect(voiceProfileStatusLabelKey({ ...base, stored_voice_profile_id: '' })).toBe('enrolledUnconfirmed');
    expect(voiceProfileStatusLabelKey({ ...base, stored_voice_profile_id: null })).toBe('enrolledUnconfirmed');
  });

  it('is none when not enrolled and explicitly inactive', () => {
    expect(voiceProfileStatusLabelKey({ ...base, is_enrolled: false, is_voice_profile_active: false })).toBe('none');
  });
});

describe('voice profile selected/active id passthrough', () => {
  it('keeps them absent when the service did not send them', () => {
    const s = normalizeVoiceProfileStatus({ is_enrolled: true });
    expect(s.voice_profile_selected).toBeUndefined();
    expect(s.active_voice_profile_id).toBeUndefined();
  });
  it('carries them through normalize and merge', () => {
    const res = { is_enrolled: true, voice_profile_selected: true, active_voice_profile_id: 'p1' };
    expect(normalizeVoiceProfileStatus(res).active_voice_profile_id).toBe('p1');
    const m = mergeVoiceProfileStatus(normalizeVoiceProfileStatus({}), res, 0);
    expect(m.voice_profile_selected).toBe(true);
    expect(m.active_voice_profile_id).toBe('p1');
  });
});

describe('stale service state on set/clear (I1)', () => {
  const local = { is_enrolled: false, active_samples_count: 0, embedding_dim: 192, neural_eq_calibrated: false };
  const activeBefore = {
    is_enrolled: true, active_samples_count: 1, embedding_dim: 192, neural_eq_calibrated: true,
    is_voice_profile_active: true, stored_voice_profile_id: 'p1', voice_profile_error: 'x',
    voice_profile_selected: true, active_voice_profile_id: 'p1',
  };
  it('strips every service key before sending', () => {
    const s = stripServiceVoiceProfileKeys(activeBefore);
    for (const k of ['is_voice_profile_active', 'stored_voice_profile_id', 'voice_profile_error', 'voice_profile_selected', 'active_voice_profile_id']) {
      expect(k in s).toBe(false);
    }
    expect(s.is_enrolled).toBe(true);
  });
  it('active -> delete last sample with service answering inactive is not active', () => {
    const res = { ...local, is_voice_profile_active: false, stored_voice_profile_id: null };
    const next = applySetVoiceProfileResult(activeBefore, local, res);
    expect(next.active_samples_count).toBe(0);
    expect(voiceProfileStatusLabelKey(next)).toBe('none');
  });
  it('result without service keys never yields active', () => {
    const next = applySetVoiceProfileResult(activeBefore, { ...local, is_enrolled: true, active_samples_count: 2 }, { is_enrolled: true });
    expect(voiceProfileStatusLabelKey(next)).toBe('enrolledUnconfirmed');
    expect(next.is_voice_profile_active).toBeUndefined();
    expect(next.stored_voice_profile_id).toBeUndefined();
  });
  it('missing/offline result clears service keys', () => {
    for (const res of [undefined, null, { success: false }]) {
      const next = applySetVoiceProfileResult(activeBefore, local, res);
      expect(voiceProfileStatusLabelKey(next)).toBe('none');
      expect(next.voice_profile_error).toBeUndefined();
    }
  });
  it('forward error surfaces and service keys replace old ones', () => {
    const res = { ...local, is_enrolled: true, active_samples_count: 1, is_voice_profile_active: false, voice_profile_error: 'service_unavailable' };
    const next = applySetVoiceProfileResult(activeBefore, { ...local, is_enrolled: true, active_samples_count: 1 }, res);
    expect(next.voice_profile_error).toBe('service_unavailable');
    expect(next.is_voice_profile_active).toBe(false);
    expect(next.active_voice_profile_id).toBeUndefined();
  });
});

describe('voiceProfileErrorKey (M5)', () => {
  it('maps known codes and service messages', () => {
    expect(voiceProfileErrorKey('service_unavailable')).toBe('errorServiceUnavailable');
    expect(voiceProfileErrorKey('service_error')).toBe('errorServiceError');
    expect(voiceProfileErrorKey('service_rejected')).toBe('errorServiceRejected');
    expect(voiceProfileErrorKey('VOICE_PROFILE_NOT_APPLICABLE')).toBe('errorNotApplicable');
    expect(voiceProfileErrorKey('VOICE_PROFILE_CLEAR_FAILED')).toBe('errorClearFailed');
    expect(voiceProfileErrorKey('NO_PROFILE_STORE')).toBe('errorNoProfileStore');
    expect(voiceProfileErrorKey('Invalid voice profile')).toBe('errorInvalidProfile');
    expect(voiceProfileErrorKey('Failed to persist voice profile')).toBe('errorPersistFailed');
    expect(voiceProfileErrorKey('The backend could not apply the stored voice profile')).toBe('errorNotApplied');
    expect(voiceProfileErrorKey('The stored voice profile failed validation or has insecure permissions')).toBe('errorLoadFailed');
    expect(voiceProfileErrorKey('The previous voice profile could not be restored on the backend')).toBe('errorRestoreFailed');
    expect(voiceProfileErrorKey('The active backend could not return to the neutral voice')).toBe('errorClearFailed');
    expect(voiceProfileErrorKey('The active backend cannot apply this voice profile')).toBe('errorNotApplicable');
    expect(voiceProfileErrorKey('Voice profile storage is not configured')).toBe('errorNoProfileStore');
  });
  it('falls back to unknown for free text and every key exists in both locales', () => {
    expect(voiceProfileErrorKey('some <b>free</b> text')).toBe('errorUnknown');
    expect(voiceProfileErrorKey('')).toBe('errorUnknown');
    const codes = ['service_unavailable', 'service_error', 'service_rejected', 'VOICE_PROFILE_NOT_APPLICABLE', 'VOICE_PROFILE_CLEAR_FAILED', 'NO_PROFILE_STORE', 'x'];
    for (const c of codes) {
      const k = voiceProfileErrorKey(c) as keyof typeof ptBR.voiceProfile;
      expect(typeof ptBR.voiceProfile[k]).toBe('string');
      expect(typeof enUS.voiceProfile[k]).toBe('string');
    }
  });
});

describe('relabel (I2/T7)', () => {
  it('active is applied-in-service with a note; enrolled uses Cadastrado', () => {
    expect(ptBR.voiceProfile.statusActive).toBe('🟢 Aplicado no serviço (desenvolvimento)');
    expect(enUS.voiceProfile.statusActive).toBe('🟢 Applied in service (development)');
    expect(ptBR.voiceProfile.appliedInServiceNote).toBe('O microfone virtual empacotado ainda não usa este perfil.');
    expect(enUS.voiceProfile.appliedInServiceNote).toBe('The packaged virtual microphone does not use this profile yet.');
    expect(ptBR.voiceProfile.enrolledUnconfirmed).toBe('⚪ Cadastrado, ativação não confirmada');
  });
});
