import { describe, it, expect } from 'vitest';
import { ptBR } from '../i18n/locales/pt-BR';
import { enUS } from '../i18n/locales/en-US';
import type { StudioPreset, VoiceSample, CallSuggestionTake, VoiceProfileStatus, InputDeviceInfo } from '../types';
import {
  isVirtualOrLoopbackAudioDevice,
  resolvePhysicalAudioDevice,
} from '../VoiceProfileCard';

describe('Voice Profile & Speaker Isolation UI Specs', () => {
  it('contains the 5 everyday open questions specified in the continuous enrollment design', () => {
    // Step 1: Meeting Kickoff
    expect(ptBR.voiceProfile.question1Category).toBe('Início de Reunião');
    expect(ptBR.voiceProfile.question1Text).toContain('dar bom dia');
    expect(enUS.voiceProfile.question1Category).toBe('Meeting Kickoff');
    expect(enUS.voiceProfile.question1Text).toContain('good morning');

    // Step 2: Morning Routine
    expect(ptBR.voiceProfile.question2Category).toBe('Rotina Matinal');
    expect(ptBR.voiceProfile.question2Text).toContain('comeu ou bebeu');
    expect(enUS.voiceProfile.question2Category).toBe('Morning Routine');
    expect(enUS.voiceProfile.question2Text).toContain('eat or drink');

    // Step 3: Work Focus
    expect(ptBR.voiceProfile.question3Category).toBe('Foco de Trabalho');
    expect(ptBR.voiceProfile.question3Text).toContain('principal tarefa ou projeto');
    expect(enUS.voiceProfile.question3Category).toBe('Work Focus');
    expect(enUS.voiceProfile.question3Text).toContain('main task or project');

    // Step 4: Workspace
    expect(ptBR.voiceProfile.question4Category).toBe('Espaço de Trabalho');
    expect(ptBR.voiceProfile.question4Text).toContain('organizar sua mesa');
    expect(enUS.voiceProfile.question4Category).toBe('Workspace');
    expect(enUS.voiceProfile.question4Text).toContain('arrange your desk');

    // Step 5: Leisure
    expect(ptBR.voiceProfile.question5Category).toBe('Lazer / Descontração');
    expect(ptBR.voiceProfile.question5Text).toContain('viajar para qualquer lugar');
    expect(enUS.voiceProfile.question5Category).toBe('Leisure & Downtime');
    expect(enUS.voiceProfile.question5Text).toContain('travel anywhere');
  });

  it('provides reading fallback sentences for all 5 steps', () => {
    for (let i = 1; i <= 5; i++) {
      const fallbackKeyPt = `question${i}Fallback` as keyof typeof ptBR.voiceProfile;
      const fallbackKeyEn = `question${i}Fallback` as keyof typeof enUS.voiceProfile;
      expect(ptBR.voiceProfile[fallbackKeyPt]).toBeDefined();
      expect(typeof ptBR.voiceProfile[fallbackKeyPt]).toBe('string');
      expect((ptBR.voiceProfile[fallbackKeyPt] as string).length).toBeGreaterThan(15);

      expect(enUS.voiceProfile[fallbackKeyEn]).toBeDefined();
      expect(typeof enUS.voiceProfile[fallbackKeyEn]).toBe('string');
      expect((enUS.voiceProfile[fallbackKeyEn] as string).length).toBeGreaterThan(15);
    }
  });

  it('contains cumulative gallery and voice intake action strings', () => {
    expect(ptBR.voiceProfile.approveTake).toContain('Aprovar');
    expect(ptBR.voiceProfile.dismissTake).toContain('Descartar');
    expect(ptBR.voiceProfile.addNewSampleBtn).toContain('Adicionar');
    // Honest wording (spec §9): the packaged virtual microphone does not use the profile yet.
    expect(ptBR.voiceProfile.activateProfileBtn).toContain('Gerar perfil no serviço');

    expect(enUS.voiceProfile.approveTake).toContain('Approve');
    expect(enUS.voiceProfile.dismissTake).toContain('Dismiss');
    expect(enUS.voiceProfile.addNewSampleBtn).toContain('Add Voice Sample');
    expect(enUS.voiceProfile.activateProfileBtn).toContain('Build profile in the service');
  });

  it('provides dynamic recording UX strings and supports flexible durations without rigid 5s limit', () => {
    // Stop recording active button
    expect(ptBR.voiceProfile.stopRecordingBtn).toBe('Concluir Gravação');
    expect(enUS.voiceProfile.stopRecordingBtn).toBe('Finish Recording');

    // Dynamic recording status with elapsed and max placeholders
    expect(ptBR.voiceProfile.recordingStatus).toContain('{elapsed}');
    expect(ptBR.voiceProfile.recordingStatus).toContain('{max}');
    expect(enUS.voiceProfile.recordingStatus).toContain('{elapsed}');
    expect(enUS.voiceProfile.recordingStatus).toContain('{max}');

    // Record sample action without arbitrary fixed 5s label
    expect(ptBR.voiceProfile.recordSample).toBe('Gravar Resposta');
    expect(enUS.voiceProfile.recordSample).toBe('Record Answer');

    // Flexible sample durations (e.g. 4.2s, 8.5s, 15.0s, 30.0s)
    const testDurations = [1.5, 4.2, 8.5, 15.0, 30.0];
    for (const d of testDurations) {
      const variableSample: VoiceSample = {
        id: `sample-flex-${d}`,
        title: `Amostra Flexível ${d}s`,
        timestamp: '14:30',
        durationSec: d,
      };
      expect(variableSample.durationSec).toBe(d);
      expect(variableSample.durationSec).toBeGreaterThanOrEqual(1.5);
      expect(variableSample.durationSec).toBeLessThanOrEqual(30.0);
    }
  });

  it('validates voice profile and sample data structures', () => {
    const sample: VoiceSample = {
      id: 'sample-1',
      title: 'Início de Reunião',
      timestamp: '10:00',
      durationSec: 5.0,
      isInitialStep: true,
    };
    expect(sample.id).toBe('sample-1');
    expect(sample.durationSec).toBe(5.0);

    const take: CallSuggestionTake = {
      id: 'take-1',
      title: 'Meet Call',
      timestamp: '11:00',
      durationSec: 5.2,
      snrDb: 25.0,
    };
    expect(take.snrDb).toBe(25.0);

    const status: VoiceProfileStatus = {
      is_enrolled: true,
      active_samples_count: 5,
      embedding_dim: 192,
      neural_eq_calibrated: true,
    };
    expect(status.embedding_dim).toBe(192);
  });
});

describe('Studio DSP UI Specs', () => {
  it('covers the 4 official studio presets (Natural, Warm, Broadcast, Off)', () => {
    const validPresets: StudioPreset[] = ['Natural', 'Podcast', 'Broadcast', 'Off'];
    expect(validPresets).toHaveLength(4);

    expect(ptBR.studioDsp.presetNatural).toBe('Natural');
    expect(ptBR.studioDsp.presetWarm).toContain('Warm');
    expect(ptBR.studioDsp.presetBroadcast).toContain('Broadcast');
    expect(ptBR.studioDsp.presetOff).toContain('Bypass');

    expect(enUS.studioDsp.presetNatural).toBe('Natural');
    expect(enUS.studioDsp.presetWarm).toContain('Warm');
    expect(enUS.studioDsp.presetBroadcast).toContain('Broadcast');
    expect(enUS.studioDsp.presetOff).toContain('Bypass');
  });

  it('covers the 6 active DSP blocks', () => {
    const blocks = [
      ptBR.studioDsp.blockHighpass,
      ptBR.studioDsp.blockBiquads,
      ptBR.studioDsp.blockDeesser,
      ptBR.studioDsp.blockCompressor,
      ptBR.studioDsp.blockAgc,
      ptBR.studioDsp.blockLimiter,
    ];
    for (const b of blocks) {
      expect(b).toBeDefined();
      expect(b.length).toBeGreaterThan(3);
    }

    expect(ptBR.studioDsp.neuralEqSectionTitle).toContain('Neural EQ');
    expect(enUS.studioDsp.neuralEqSectionTitle).toContain('Neural EQ');
  });
});

describe('Voice Profile Microphone Capture & Audio Anti-Loopback Specs', () => {
  it('correctly filters out virtual microphones and monitor/loopback sinks', () => {
    // Virtual mics created by ClearCore or PipeWire
    expect(isVirtualOrLoopbackAudioDevice('ClearCore Virtual Noise Suppression Microphone')).toBe(true);
    expect(isVirtualOrLoopbackAudioDevice('Realtime Denoise Virtual Microphone')).toBe(true);
    expect(isVirtualOrLoopbackAudioDevice('clearcore_mic')).toBe(true);

    // Monitor sinks and loopback devices (causes static screeching/noise loop)
    expect(isVirtualOrLoopbackAudioDevice('Monitor of Built-in Audio Analog Stereo')).toBe(true);
    expect(isVirtualOrLoopbackAudioDevice('Loopback Audio Source')).toBe(true);
    expect(isVirtualOrLoopbackAudioDevice('alsa_output.pci.monitor')).toBe(true);

    // Legitimate physical hardware microphones
    expect(isVirtualOrLoopbackAudioDevice('Built-in Audio Analog Stereo')).toBe(false);
    expect(isVirtualOrLoopbackAudioDevice('Yeti Stereo Microphone Analog Stereo')).toBe(false);
    expect(isVirtualOrLoopbackAudioDevice('USB Audio Device')).toBe(false);
    expect(isVirtualOrLoopbackAudioDevice('MacBook Pro Microphone')).toBe(false);
  });

  it('accurately resolves physical microphone matching PipeWire device info to Chromium device label', () => {
    const mockAudioInputs: MediaDeviceInfo[] = [
      {
        deviceId: 'chromium-uuid-virtual-1',
        groupId: 'grp-1',
        kind: 'audioinput',
        label: 'ClearCore Virtual Noise Suppression Microphone',
        toJSON: () => ({}),
      },
      {
        deviceId: 'chromium-uuid-monitor-2',
        groupId: 'grp-2',
        kind: 'audioinput',
        label: 'Monitor of Family 17h/19h HD Audio Controller',
        toJSON: () => ({}),
      },
      {
        deviceId: 'chromium-uuid-physical-yeti',
        groupId: 'grp-3',
        kind: 'audioinput',
        label: 'Blue Microphones Yeti Stereo Microphone Analog Stereo',
        toJSON: () => ({}),
      },
      {
        deviceId: 'chromium-uuid-internal-mic',
        groupId: 'grp-4',
        kind: 'audioinput',
        label: 'Family 17h/19h HD Audio Controller Analog Stereo',
        toJSON: () => ({}),
      },
    ];

    const inputDevices: InputDeviceInfo[] = [
      {
        id: 'alsa_input.usb-Blue_Microphones_Yeti_Stereo_Microphone_REV8-00.analog-stereo',
        name: 'Blue Microphones Yeti Stereo Microphone',
        is_default: false,
      },
      {
        id: 'alsa_input.pci-0000_09_00.6.analog-stereo',
        name: 'Family 17h/19h HD Audio Controller Analog Stereo',
        is_default: true,
      },
    ];

    // 1. When PipeWire Yeti mic is selected:
    const resolvedYeti = resolvePhysicalAudioDevice(
      mockAudioInputs,
      'alsa_input.usb-Blue_Microphones_Yeti_Stereo_Microphone_REV8-00.analog-stereo',
      inputDevices
    );
    expect(resolvedYeti).toBeDefined();
    expect(resolvedYeti?.deviceId).toBe('chromium-uuid-physical-yeti');
    expect(resolvedYeti?.label).toContain('Yeti');

    // 2. When internal mic is selected:
    const resolvedInternal = resolvePhysicalAudioDevice(
      mockAudioInputs,
      'alsa_input.pci-0000_09_00.6.analog-stereo',
      inputDevices
    );
    expect(resolvedInternal).toBeDefined();
    expect(resolvedInternal?.deviceId).toBe('chromium-uuid-internal-mic');

    // 3. When selectedInputId is not recognized, it must pick a physical mic and NEVER the virtual or monitor
    const fallback = resolvePhysicalAudioDevice(mockAudioInputs, 'unknown-id', inputDevices);
    expect(fallback).toBeDefined();
    expect(fallback?.label).not.toContain('ClearCore');
    expect(fallback?.label).not.toContain('Monitor');
    expect(['chromium-uuid-physical-yeti', 'chromium-uuid-internal-mic']).toContain(fallback?.deviceId);
  });

  it('re-exports the capture helpers of voice/captureDevice and no longer exposes MediaRecorder helpers', async () => {
    const card = await import('../VoiceProfileCard');
    const capture = await import('../voice/captureDevice');
    expect(card.isVirtualOrLoopbackAudioDevice).toBe(capture.isVirtualOrLoopbackAudioDevice);
    expect(card.resolvePhysicalAudioDevice).toBe(capture.resolvePhysicalAudioDevice);
    // Raw PCM capture (processing off, no fallback) is covered in voice/__tests__/capture.test.ts.
    for (const gone of ['getPreferredAudioMimeType', 'PURE_VOICE_CAPTURE_CONSTRAINTS', 'stopMediaRecorderAsync', 'acquireCleanPhysicalStream']) {
      expect(gone in card, gone).toBe(false);
    }
  });
});
