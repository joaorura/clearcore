import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  MicPermissionDeniedError,
  PhysicalMicUnavailableError,
  acquireRawPhysicalStream,
  resolvePhysicalAudioDevice,
} from '../captureDevice';
import { PcmRecorder } from '../pcmCapture';

afterEach(() => vi.unstubAllGlobals());

const track = (label = 'Blue Yeti') => ({ label, stop: vi.fn(), getSettings: () => ({}) });
const streamOf = (...tracks: ReturnType<typeof track>[]) =>
  ({ getTracks: () => tracks, getAudioTracks: () => tracks }) as unknown as MediaStream;
const dev = (label: string, deviceId: string, groupId = 'g') => ({ kind: 'audioinput', label, deviceId, groupId });

function stubMedia(devices: unknown[] | (() => unknown[]), gum: ReturnType<typeof vi.fn>) {
  vi.stubGlobal('navigator', {
    mediaDevices: { enumerateDevices: async () => (typeof devices === 'function' ? devices() : devices), getUserMedia: gum },
  });
}

describe('stream ownership in acquireRawPhysicalStream', () => {
  it('does not open the device when hashing fails', async () => {
    const gum = vi.fn(async (_c?: unknown) => streamOf(track()));
    stubMedia([dev('Blue Yeti', 'abc')], gum);
    vi.spyOn(crypto.subtle, 'digest').mockRejectedValueOnce(new Error('boom'));
    await expect(acquireRawPhysicalStream()).rejects.toThrow();
    expect(gum).not.toHaveBeenCalled();
  });
});

describe('alias devices', () => {
  it('opens a lone "default" alias and validates the track label', async () => {
    const t = track('Default - Mic USB');
    const gum = vi.fn(async (_c?: unknown) => streamOf(t));
    stubMedia([dev('Default - Mic USB', 'default')], gum);
    const r = await acquireRawPhysicalStream();
    expect((gum.mock.calls[0][0] as { audio: { deviceId: unknown } }).audio.deviceId).toEqual({ exact: 'default' });
    expect(r.device.label).toBe('Default - Mic USB');
    expect(t.stop).not.toHaveBeenCalled();
  });
  it('refuses and stops the track when the alias resolves to a virtual device', async () => {
    const t = track('Default - realtime-noise-source');
    stubMedia([dev('Default', 'default')], vi.fn(async () => streamOf(t)));
    await expect(acquireRawPhysicalStream()).rejects.toBeInstanceOf(PhysicalMicUnavailableError);
    expect(t.stop).toHaveBeenCalled();
  });
  it('prefers a non-alias with the same group', async () => {
    const gum = vi.fn(async (_c?: unknown) => streamOf(track()));
    stubMedia([dev('Default - Mic USB', 'default', 'g1'), dev('Mic USB', 'usb', 'g1')], gum);
    await acquireRawPhysicalStream();
    expect((gum.mock.calls[0][0] as { audio: { deviceId: unknown } }).audio.deviceId).toEqual({ exact: 'usb' });
  });
});

describe('label probe', () => {
  it('does not probe when labels are already available', async () => {
    const gum = vi.fn(async (_c?: unknown) => streamOf(track()));
    stubMedia([dev('Blue Yeti', 'abc')], gum);
    await acquireRawPhysicalStream();
    expect(gum).toHaveBeenCalledTimes(1);
  });
  it('probes when labels are empty and stops the probe tracks immediately', async () => {
    const probe = track();
    let granted = false;
    const gum = vi.fn(async (c: { audio: unknown }) => { if (c.audio === true) { granted = true; return streamOf(probe); } return streamOf(track()); });
    stubMedia(() => [dev(granted ? 'Blue Yeti' : '', 'abc')], gum);
    await acquireRawPhysicalStream();
    expect(probe.stop).toHaveBeenCalled();
    expect(gum).toHaveBeenCalledTimes(2);
  });
  it('reports a denied permission distinguishably', async () => {
    const denied = Object.assign(new Error('denied'), { name: 'NotAllowedError' });
    stubMedia([dev('', 'abc')], vi.fn(async () => { throw denied; }));
    await expect(acquireRawPhysicalStream()).rejects.toBeInstanceOf(MicPermissionDeniedError);
  });
});

describe('resolvePhysicalAudioDevice', () => {
  it('does not match inputDevices entries without id when no input is selected', () => {
    const inputs = [dev('Other', 'o'), dev('Mic USB', 'usb')] as unknown as MediaDeviceInfo[];
    const r = resolvePhysicalAudioDevice(inputs, undefined, [{ name: 'Mic USB' }]);
    expect(r?.deviceId).toBe('o'); // step 3 (first labelled), not the undefined===undefined name match
  });
});

class FakeContext {
  static last: FakeContext;
  static failModule = false;
  sampleRate = 48000; state = 'running';
  close = vi.fn(async () => { this.state = 'closed'; });
  resume = vi.fn(async () => undefined);
  audioWorklet = { addModule: vi.fn(async () => { if (FakeContext.failModule) throw new Error('csp'); }) };
  createMediaStreamSource = vi.fn(() => ({ connect: vi.fn(), disconnect: vi.fn() }));
  constructor() { FakeContext.last = this; }
}
class FakeNode { port: { onmessage: unknown } = { onmessage: null }; disconnect = vi.fn(); }

describe('PcmRecorder resource safety', () => {
  const setup = () => {
    FakeContext.failModule = false;
    vi.stubGlobal('AudioContext', FakeContext);
    vi.stubGlobal('AudioWorkletNode', FakeNode);
  };
  it('closes the context and stops the tracks when addModule rejects', async () => {
    setup();
    const t = track();
    FakeContext.failModule = true;
    const rec = new PcmRecorder();
    const pending = rec.start(streamOf(t));
    await expect(pending).rejects.toThrow('csp');
    expect(FakeContext.last.close).toHaveBeenCalled();
    expect(t.stop).toHaveBeenCalled();
  });
  it('stops the tracks when AudioContext cannot be created', async () => {
    vi.stubGlobal('AudioContext', class { constructor() { throw new Error('no audio'); } });
    const t = track();
    await expect(new PcmRecorder().start(streamOf(t))).rejects.toThrow('no audio');
    expect(t.stop).toHaveBeenCalled();
  });
  it('tears everything down when stop() happens during start()', async () => {
    setup();
    const t = track();
    const rec = new PcmRecorder();
    const pending = rec.start(streamOf(t));
    await rec.stop({ label: 'x', idHash: 'h' });
    await pending;
    expect(FakeContext.last.close).toHaveBeenCalled();
    expect(t.stop).toHaveBeenCalled();
  });
  it('stops recording and calls onEnded when the track ends', async () => {
    setup();
    let ended: (() => void) | null = null;
    const t = { ...track(), addEventListener: (_: string, cb: () => void) => { ended = cb; } };
    const onEnded = vi.fn();
    await new PcmRecorder().start(streamOf(t as never), undefined, onEnded);
    ended!();
    expect(onEnded).toHaveBeenCalled();
  });
  it('exposes truncated after the duration guard', async () => {
    setup();
    const rec = new PcmRecorder({ maxSeconds: 0.001 }); // 48 samples
    await rec.start(streamOf(track()));
    const node = (rec as unknown as { node: FakeNode }).node;
    (node.port.onmessage as (e: { data: Float32Array }) => void)({ data: new Float32Array(128).fill(0.1) });
    expect(rec.truncated).toBe(true);
    const out = await rec.stop({ label: 'x', idHash: 'h' });
    expect(out.pcm.length).toBe(48);
  });
});
