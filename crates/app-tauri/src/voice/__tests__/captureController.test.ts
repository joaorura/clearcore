import { afterEach, describe, expect, it, vi } from 'vitest';
import { CaptureController, type RecorderLike } from '../captureController';
import { PcmRecorder } from '../pcmCapture';
import type { CapturedPcm, DeviceInfo } from '../enrollmentTypes';

afterEach(() => vi.unstubAllGlobals());

const device: DeviceInfo = { label: 'Mic', idHash: 'h' };
const mkStream = () => {
  const t = { stop: vi.fn(), addEventListener: vi.fn() };
  return { stream: { getTracks: () => [t] } as unknown as MediaStream, t };
};
const deferred = <T,>() => {
  let resolve!: (v: T) => void;
  const promise = new Promise<T>((r) => { resolve = r; });
  return { promise, resolve };
};
const fakeRecorder = (startImpl: () => Promise<void> = async () => undefined) => {
  const stop = vi.fn(async () => ({ pcm: new Float32Array(2).fill(1) }) as unknown as CapturedPcm);
  const rec: RecorderLike = { start: vi.fn(startImpl), stop };
  return { rec, stop };
};

describe('CaptureController single-flight', () => {
  it('two calls without await acquire only once', async () => {
    const d = deferred<{ stream: MediaStream; device: DeviceInfo }>();
    const acquire = vi.fn(() => d.promise);
    const { rec } = fakeRecorder();
    const c = new CaptureController(() => rec);
    const first = c.start({ acquire });
    const second = await c.start({ acquire });
    expect(second.status).toBe('busy');
    expect(c.isStarting).toBe(true);
    const s = mkStream();
    d.resolve({ stream: s.stream, device });
    expect((await first).status).toBe('started');
    expect(acquire).toHaveBeenCalledTimes(1);
    expect(c.isStarting).toBe(false);
  });

  it('cancel during acquire stops the tracks and never starts a recorder', async () => {
    const d = deferred<{ stream: MediaStream; device: DeviceInfo }>();
    const create = vi.fn(() => fakeRecorder().rec);
    const c = new CaptureController(create);
    const p = c.start({ acquire: () => d.promise });
    c.cancel();
    const s = mkStream();
    d.resolve({ stream: s.stream, device });
    expect((await p).status).toBe('cancelled');
    expect(s.t.stop).toHaveBeenCalled();
    expect(create).not.toHaveBeenCalled();
  });

  it('cancel during recorder.start discards the take', async () => {
    const d = deferred<void>();
    const { rec, stop } = fakeRecorder(() => d.promise);
    const c = new CaptureController(() => rec);
    const s = mkStream();
    const p = c.start({ acquire: async () => ({ stream: s.stream, device }) });
    await Promise.resolve(); await Promise.resolve();
    c.cancel();
    d.resolve();
    expect((await p).status).toBe('cancelled');
    expect(stop).toHaveBeenCalled();
  });

  it('a failing PcmRecorder.start closes the AudioContext and stops the track', async () => {
    const close = vi.fn(async () => undefined);
    vi.stubGlobal('AudioContext', class {
      sampleRate = 48000; state = 'running'; close = close;
      audioWorklet = { addModule: async () => { throw new Error('csp'); } };
    });
    const s = mkStream();
    const c = new CaptureController(() => new PcmRecorder());
    const r = await c.start({ acquire: async () => ({ stream: s.stream, device }) });
    expect(r.status).toBe('failed');
    expect(close).toHaveBeenCalled();
    expect(s.t.stop).toHaveBeenCalled();
    expect(c.isActive).toBe(false);
  });

  it('stop returns the PCM and clears the state', async () => {
    const { rec } = fakeRecorder();
    const c = new CaptureController(() => rec);
    await c.start({ acquire: async () => ({ stream: mkStream().stream, device }) });
    expect(c.isActive).toBe(true);
    expect(await c.stop()).not.toBeNull();
    expect(c.isActive).toBe(false);
    expect(await c.stop()).toBeNull();
  });
});
