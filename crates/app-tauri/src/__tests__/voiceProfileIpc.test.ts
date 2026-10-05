import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { invokeBridge } from '../bridge';
import { normalizeVoiceProfileStatus, mergeVoiceProfileStatus } from '../VoiceProfileCard';
import type { VoiceProfileStatus, VoiceSample, CallSuggestionTake, StudioPreset } from '../types';
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
    expect(profile.embedding_dim).toBe(192);
    expect(profile.gain_boost_db).toBe(1.8);
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

  it('manages voice samples (add, list, delete)', () => {
    const sample1 = {
      id: 'sample-test-1',
      title: 'Amostra Teste 1',
      category: 'Início de Reunião',
      timestamp: '10:00',
      durationSec: 5.0,
      isInitialStep: true,
    };

    const addRes = voiceProfileStore.addVoiceSample(sample1, testDir);
    expect(addRes.sample.id).toBe('sample-test-1');
    expect(addRes.samples).toHaveLength(1);

    const list1 = voiceProfileStore.readVoiceSamples(testDir);
    expect(list1).toHaveLength(1);
    expect(list1[0].id).toBe('sample-test-1');

    // Profile auto-updates active_samples_count
    const profile = voiceProfileStore.readVoiceProfile(testDir);
    expect(profile.active_samples_count).toBe(1);
    expect(profile.is_enrolled).toBe(true);

    // Delete sample
    const delRes = voiceProfileStore.deleteVoiceSample('sample-test-1', testDir);
    expect(delRes.success).toBe(true);
    expect(voiceProfileStore.readVoiceSamples(testDir)).toHaveLength(0);

    const profileAfterDel = voiceProfileStore.readVoiceProfile(testDir);
    expect(profileAfterDel.active_samples_count).toBe(0);
    expect(profileAfterDel.is_enrolled).toBe(false);
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
    const approveRes = voiceProfileStore.approveCallTake('take-meet-1', 'Meet Validado', take1, testDir);
    expect(approveRes.success).toBe(true);
    expect(approveRes.takes).toHaveLength(0);
    expect(approveRes.sample.title).toBe('Chamada Google Meet');
    expect(voiceProfileStore.readVoiceSamples(testDir)).toHaveLength(1);

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
    const mockGetSamples = vi.fn().mockResolvedValue([{ id: 's1', title: 'T1' }]);
    const mockAddSample = vi.fn().mockResolvedValue({ success: true });
    const mockDeleteSample = vi.fn().mockResolvedValue({ success: true, id: 's1' });
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
      getVoiceSamples: mockGetSamples,
      addVoiceSample: mockAddSample,
      deleteVoiceSample: mockDeleteSample,
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

    // 3. get_voice_samples
    const samplesRes = await invokeBridge<VoiceSample[]>('get_voice_samples');
    expect(samplesRes).toHaveLength(1);
    expect(mockGetSamples).toHaveBeenCalled();

    // 4. add_voice_sample
    await invokeBridge('add_voice_sample', { sample: { id: 's2' } });
    expect(mockAddSample).toHaveBeenCalled();

    // 5. delete_voice_sample
    await invokeBridge('delete_voice_sample', { id: 's1' });
    expect(mockDeleteSample).toHaveBeenCalledWith('s1');

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
    expect(profileRes.embedding_dim).toBe(192);

    const setRes = await invokeBridge<{ success: boolean }>('set_voice_profile', {
      profile: { is_enrolled: true, active_samples_count: 5 },
    });
    expect(setRes.success).toBe(true);

    const presetRes = await invokeBridge<StudioPreset>('get_studio_preset');
    expect(presetRes).toBe('Natural');

    const setPresetRes = await invokeBridge<{ success: boolean; preset: string }>('set_studio_preset', {
      preset: 'Broadcast',
    });
    expect(setPresetRes.success).toBe(true);
    expect(setPresetRes.preset).toBe('Broadcast');

    const samples = await invokeBridge<VoiceSample[]>('get_voice_samples');
    expect(Array.isArray(samples)).toBe(true);

    const addRes = await invokeBridge<{ success: boolean }>('add_voice_sample', {
      sample: { id: 'test' },
    });
    expect(addRes.success).toBe(true);

    const delRes = await invokeBridge<{ success: boolean }>('delete_voice_sample', { id: 'test' });
    expect(delRes.success).toBe(true);
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
