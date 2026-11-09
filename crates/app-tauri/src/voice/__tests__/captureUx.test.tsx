import { afterEach, describe, expect, it, vi } from 'vitest';
import { renderToStaticMarkup as html } from 'react-dom/server';
import { displayDeviceLabel, resolvePhysicalAudioDevice } from '../captureDevice';
import { PcmRecorder } from '../pcmCapture';
import { VuMeter, JobFeedbackBlock } from '../panels/shared';
import { ProfilePanel, type ProfilePanelProps } from '../panels/ProfilePanel';
import { EnrollmentJobStatus } from '../EnrollmentJobStatus';
import type { EnrollmentJob, EnrollmentLabels } from '../enrollmentTypes';

afterEach(() => vi.unstubAllGlobals());

const dev = (label: string, deviceId: string, groupId = 'g') =>
  ({ kind: 'audioinput', label, deviceId, groupId }) as unknown as MediaDeviceInfo;
const t = (k: string, p?: Record<string, string | number>) => (p ? `${k}${JSON.stringify(p)}` : k);

describe('microphone fallback goes to the system default, not the first labelled device', () => {
  const inputs = [dev('Headset Microphone', 'hs', 'g1'), dev('Default - Webcam Mic', 'default', 'g2'), dev('Webcam Mic', 'wc', 'g2')];
  it('uses the "default" alias when the selection does not match', () => {
    expect(resolvePhysicalAudioDevice(inputs, 'unknown-id')?.deviceId).toBe('default');
    expect(resolvePhysicalAudioDevice(inputs, undefined)?.deviceId).toBe('default');
  });
  it('uses the device flagged is_default by the service when there is no alias', () => {
    const noAlias = [dev('Headset Microphone', 'hs', 'g1'), dev('Webcam Mic', 'wc', 'g2')];
    const r = resolvePhysicalAudioDevice(noAlias, 'unknown-id', [
      { id: 'a', name: 'Headset Microphone', is_default: false },
      { id: 'b', name: 'Webcam Mic', is_default: true },
    ]);
    expect(r?.deviceId).toBe('wc');
  });
  it('never touches an explicit saved selection that matches', () => {
    expect(resolvePhysicalAudioDevice(inputs, 'hs')?.deviceId).toBe('hs');
    expect(resolvePhysicalAudioDevice(inputs, 'x', [{ id: 'x', name: 'Headset Microphone' }])?.deviceId).toBe('hs');
  });
  it('still skips a default alias that is a virtual device', () => {
    const virt = [dev('Default - ClearCore Virtual Mic', 'default', 'g9'), dev('Webcam Mic', 'wc', 'g2')];
    expect(resolvePhysicalAudioDevice(virt, 'nope')?.deviceId).toBe('wc');
  });
  it('displayDeviceLabel drops the alias prefix and keeps the case', () => {
    expect(displayDeviceLabel('Default - Headset Microphone')).toBe('Headset Microphone');
    expect(displayDeviceLabel('Padrão: Mic USB')).toBe('Mic USB');
    expect(displayDeviceLabel(undefined)).toBe('');
  });
});

describe('PcmRecorder level callback', () => {
  it('reports peak, RMS and length of each chunk', async () => {
    class Ctx {
      sampleRate = 48000; state = 'running';
      close = vi.fn(async () => undefined); resume = vi.fn(async () => undefined);
      audioWorklet = { addModule: vi.fn(async () => undefined) };
      createMediaStreamSource = vi.fn(() => ({ connect: vi.fn(), disconnect: vi.fn() }));
    }
    let node: { port: { onmessage: ((e: { data: Float32Array }) => void) | null } } | null = null;
    class Node { port = { onmessage: null as ((e: { data: Float32Array }) => void) | null }; disconnect = vi.fn(); constructor() { node = this; } }
    vi.stubGlobal('AudioContext', Ctx);
    vi.stubGlobal('AudioWorkletNode', Node);
    vi.stubGlobal('URL', Object.assign(URL, { createObjectURL: () => 'blob:x', revokeObjectURL: () => undefined }));
    const calls: unknown[][] = [];
    const stream = { getTracks: () => [] } as unknown as MediaStream;
    await new PcmRecorder().start(stream, (...a) => calls.push(a));
    const chunk = new Float32Array(128).fill(0.1);
    chunk[3] = -0.5;
    node!.port.onmessage!({ data: chunk });
    expect(calls).toHaveLength(1);
    expect(calls[0][0]).toBe(0.5);
    expect(calls[0][1] as number).toBeGreaterThan(0.1);
    expect(calls[0][2]).toBe(128);
  });
});

describe('VuMeter extras', () => {
  it('shows the open microphone above the meter and the hint only while recording', () => {
    const rec = html(<VuMeter t={t} isRecording level={40} micLabel="Webcam Mic" lowLevelHint />);
    expect(rec).toContain('voiceProfile.micInUse{&quot;label&quot;:&quot;Webcam Mic&quot;}');
    expect(rec).toContain('voiceProfile.speakLouderHint');
    expect(rec.indexOf('micInUse')).toBeLessThan(rec.indexOf('voiceProfile.voiceLevel'));
    const idle = html(<VuMeter t={t} isRecording={false} level={40} micLabel="Webcam Mic" lowLevelHint />);
    expect(idle).not.toContain('micInUse');
    expect(idle).not.toContain('speakLouderHint');
  });
  it('shows the level percent as given (dBFS-mapped upstream)', () => {
    expect(html(<VuMeter t={t} isRecording level={67} />)).toContain('67%');
  });
});

const labels = { errors: {}, stageEnroll: 'Gerando perfil' } as unknown as EnrollmentLabels;
const running: EnrollmentJob = { jobId: 'profile-job-1', state: 'running', stage: 'enroll', errorCode: null, remainingSeconds: null, sampleId: null, profileId: null, quality: null };

describe('loading feedback while a job runs', () => {
  it('JobFeedbackBlock shows a spinner and the busy label (not "sending sample") for a build', () => {
    const m = html(<JobFeedbackBlock t={t} labels={labels} locale="pt-BR" feedback={{ jobBusy: true, currentJob: null, enrollErrorText: null, busyLabel: 'voiceProfile.buildingProfile' }} />);
    expect(m).toContain('data-testid="job-spinner"');
    expect(m).toContain('voiceProfile.buildingProfile');
    expect(m).not.toContain('voiceProfile.sendingSample');
  });
  it('JobFeedbackBlock keeps the sample text by default', () => {
    const m = html(<JobFeedbackBlock t={t} labels={labels} locale="pt-BR" feedback={{ jobBusy: true, currentJob: null, enrollErrorText: null }} />);
    expect(m).toContain('voiceProfile.sendingSample');
  });
  it('a running job shows its stage with a spinner', () => {
    const m = html(<EnrollmentJobStatus job={running} labels={labels} />);
    expect(m).toContain('data-testid="job-spinner"');
    expect(m).toContain('Gerando perfil');
  });
  const profile = (over: Partial<ProfilePanelProps>): ProfilePanelProps => ({
    t, locale: 'pt-BR', labels, profileStatus: { is_enrolled: false, active_samples_count: 2 }, samplesCount: 2,
    canBuild: true, busy: false, feedback: null, onBuildProfile: () => undefined, ...over,
  });
  it('the build button is disabled with the building text while building', () => {
    const m = html(<ProfilePanel {...profile({ isBuilding: true, busy: true })} />);
    expect(m).toMatch(/<button[^>]*class="action-btn"[^>]*disabled=""[^>]*>⏳ voiceProfile\.buildingProfile/);
  });
  it('says "processing sample" while the build waits for a sample', () => {
    const m = html(<ProfilePanel {...profile({ isBuilding: true, waitingForSample: true })} />);
    expect(m).toMatch(/<button[^>]*disabled=""[^>]*>⏳ voiceProfile\.processingSample/);
    expect(m).not.toContain('voiceProfile.buildingProfile');
  });
  it('the panel shows the spinner through the job feedback while building', () => {
    const feedback = { jobBusy: true, currentJob: null, enrollErrorText: null, busyLabel: 'voiceProfile.buildingProfile' };
    expect(html(<ProfilePanel {...profile({ isBuilding: true, busy: true, feedback })} />)).toContain('job-spinner');
    expect(html(<ProfilePanel {...profile({})} />)).not.toContain('job-spinner');
  });
});
