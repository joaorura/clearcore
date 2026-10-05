import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  PhysicalMicUnavailableError,
  acquireRawPhysicalStream,
  isVirtualOrLoopbackAudioDevice,
} from '../captureDevice';
import { concatChunks, measurePcm } from '../pcmCapture';

afterEach(() => vi.unstubAllGlobals());

describe('pcm helpers', () => {
  it('measurePcm reports peak and rms', () => {
    const m = measurePcm(new Float32Array([0.5, -0.5, 0.5, -0.5]));
    expect(m.peak).toBeCloseTo(0.5);
    expect(m.rmsDbfs).toBeCloseTo(-6.02, 1);
    expect(measurePcm(new Float32Array(4)).rmsDbfs).toBe(-120);
  });
  it('concatChunks joins in order', () => {
    expect([...concatChunks([new Float32Array([1, 2]), new Float32Array([3])])]).toEqual([1, 2, 3]);
  });
});

describe('device selection', () => {
  it('virtual and loopback labels are excluded', () => {
    for (const l of ['ClearCore Virtual Mic', 'Monitor of Built-in', 'Loopback', 'realtime-noise'])
      expect(isVirtualOrLoopbackAudioDevice(l)).toBe(true);
    expect(isVirtualOrLoopbackAudioDevice('Blue Yeti')).toBe(false);
  });

  it('refuses when only virtual devices exist and never calls the unconstrained getUserMedia', async () => {
    const gum = vi.fn();
    vi.stubGlobal('navigator', {
      mediaDevices: {
        enumerateDevices: async () => [{ kind: 'audioinput', label: 'ClearCore Virtual Mic', deviceId: 'v', groupId: 'g' }],
        getUserMedia: gum,
      },
    });
    await expect(acquireRawPhysicalStream()).rejects.toBeInstanceOf(PhysicalMicUnavailableError);
    expect((gum.mock.calls as unknown as Array<[{ audio?: { deviceId?: unknown } }]>).every(([c]) => c?.audio?.deviceId)).toBe(true);
  });

  it('does not fall back when the exact device fails to open', async () => {
    const gum = vi.fn(async () => { throw new Error('busy'); });
    vi.stubGlobal('navigator', {
      mediaDevices: {
        enumerateDevices: async () => [{ kind: 'audioinput', label: 'Blue Yeti', deviceId: 'abc', groupId: 'grp' }],
        getUserMedia: gum,
      },
    });
    await expect(acquireRawPhysicalStream()).rejects.toBeInstanceOf(PhysicalMicUnavailableError);
    expect(gum).toHaveBeenCalledTimes(1);
  });

  it('opens the physical device with all processing off and returns a stable hash', async () => {
    const gum = vi.fn(async () => ({ getTracks: () => [] }) as unknown as MediaStream);
    vi.stubGlobal('navigator', {
      mediaDevices: {
        enumerateDevices: async () => [{ kind: 'audioinput', label: 'Blue Yeti', deviceId: 'abc', groupId: 'grp' }],
        getUserMedia: gum,
      },
    });
    const a = await acquireRawPhysicalStream();
    const b = await acquireRawPhysicalStream();
    expect(a.device.idHash).toMatch(/^[0-9a-f]{64}$/);
    expect(a.device.idHash).toBe(b.device.idHash);
    expect(a.device.label).toBe('Blue Yeti');
    const calls = gum.mock.calls as unknown as Array<[{ audio: MediaTrackConstraints }]>;
    const c = calls[calls.length - 1][0].audio;
    expect([c.echoCancellation, c.noiseSuppression, c.autoGainControl]).toEqual([false, false, false]);
    expect(c.deviceId).toEqual({ exact: 'abc' });
    expect(c.channelCount).toBe(1);
  });
});
