import { describe, it, expect } from 'vitest';
import type { VoiceProfileStatus } from '../../types';
import {
  applySetVoiceProfileResult,
  mergeVoiceProfileStatus,
  normalizeVoiceProfileStatus,
  stripServiceVoiceProfileKeys,
  voiceProfileUnsupported,
} from '../hooks/voiceProfileLogic';
import { neuralEqCalibratedFrom } from '../../StudioDspCard';

const initial: VoiceProfileStatus = { is_enrolled: false, active_samples_count: 0 };

describe('voice_profile_supported (GetStatus)', () => {
  it('is kept only as a real boolean; absent or malformed means unknown', () => {
    expect(normalizeVoiceProfileStatus({ voice_profile_supported: false }).voice_profile_supported).toBe(false);
    expect(normalizeVoiceProfileStatus({ voice_profile_supported: true }).voice_profile_supported).toBe(true);
    expect(normalizeVoiceProfileStatus({}).voice_profile_supported).toBeUndefined();
    expect(normalizeVoiceProfileStatus({ voice_profile_supported: 'false' }).voice_profile_supported).toBeUndefined();
  });

  it('only an explicit false counts as unsupported', () => {
    expect(voiceProfileUnsupported({ voice_profile_supported: false })).toBe(true);
    expect(voiceProfileUnsupported({ voice_profile_supported: true })).toBe(false);
    expect(voiceProfileUnsupported({})).toBe(false);
    expect(voiceProfileUnsupported(null)).toBe(false);
  });

  it('flows through the merge and the set_voice_profile result from the service only', () => {
    const merged = mergeVoiceProfileStatus(initial, { is_voice_profile_active: false, voice_profile_supported: false }, 0);
    expect(merged.voice_profile_supported).toBe(false);
    const applied = applySetVoiceProfileResult(
      { ...initial, voice_profile_supported: false },
      { ...initial, voice_profile_supported: false },
      { stored_voice_profile_id: null },
    );
    expect(applied.voice_profile_supported).toBeUndefined();
    expect('voice_profile_supported' in stripServiceVoiceProfileKeys({ ...initial, voice_profile_supported: true })).toBe(false);
  });
});

describe('neural_eq_calibrated (GetStatus, applied profile has EQ)', () => {
  it('comes from the service status and reaches the Studio DSP card', () => {
    const merged = mergeVoiceProfileStatus(initial, { is_voice_profile_active: true, neural_eq_calibrated: true }, 0);
    expect(merged.neural_eq_calibrated).toBe(true);
    expect(neuralEqCalibratedFrom(merged)).toBe(true);
    const notCalibrated = mergeVoiceProfileStatus(initial, { is_voice_profile_active: true, neural_eq_calibrated: false }, 0);
    expect(neuralEqCalibratedFrom(notCalibrated)).toBe(false);
  });

  it('is never kept from the local status when the service answer lacks it', () => {
    const local = { ...initial, neural_eq_calibrated: true };
    const applied = applySetVoiceProfileResult(local, local, { stored_voice_profile_id: 'p1' });
    expect(applied.neural_eq_calibrated).toBe(false);
    expect(neuralEqCalibratedFrom(applied)).toBe(false);
    expect('neural_eq_calibrated' in stripServiceVoiceProfileKeys(local)).toBe(false);
  });
});

describe('dev_base_model (GetStatus, development pDFNet3)', () => {
  it('accepts only the documented values and fixed DEV_MODEL_* codes', () => {
    expect(normalizeVoiceProfileStatus({ dev_base_model: 'pdfnet3-dev' }).dev_base_model).toBe('pdfnet3-dev');
    expect(normalizeVoiceProfileStatus({ dev_base_model: 'base' }).dev_base_model).toBe('base');
    for (const v of ['approved', 'PDFNET3-DEV', 1, null, {}]) {
      expect(normalizeVoiceProfileStatus({ dev_base_model: v }).dev_base_model).toBeUndefined();
    }
    expect(normalizeVoiceProfileStatus({ dev_base_model_error: 'DEV_MODEL_HASH_MISMATCH' }).dev_base_model_error).toBe('DEV_MODEL_HASH_MISMATCH');
    expect(normalizeVoiceProfileStatus({ dev_base_model_error: null }).dev_base_model_error).toBeNull();
    for (const v of ['/home/x/pdfnet3.tar.gz', 'DEV_MODEL_hash', 'ENROLL_FAILED', 3]) {
      expect(normalizeVoiceProfileStatus({ dev_base_model_error: v }).dev_base_model_error).toBeUndefined();
    }
  });
  it('is a service field: merged from GetStatus and never inherited by the renderer', () => {
    const merged = mergeVoiceProfileStatus(
      { is_enrolled: false, active_samples_count: 0 },
      { stored_voice_profile_id: null, dev_base_model: 'pdfnet3-dev', dev_base_model_error: null },
      0,
    );
    expect(merged.dev_base_model).toBe('pdfnet3-dev');
    expect(stripServiceVoiceProfileKeys(merged).dev_base_model).toBeUndefined();
  });
});
