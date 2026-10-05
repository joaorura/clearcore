import type { CapturedPcm, DeviceInfo } from './enrollmentTypes';

export interface RecorderLike {
  start(stream: MediaStream, onLevel?: (level01: number) => void, onEnded?: () => void): Promise<void>;
  stop(device: DeviceInfo): Promise<CapturedPcm>;
}

export type StartResult =
  | { status: 'started' }
  | { status: 'busy' }
  | { status: 'cancelled' }
  | { status: 'failed'; error: unknown };

/**
 * Single-flight, cancellable capture lifecycle (no React). A generation token makes any acquisition
 * that finishes after cancel()/another start() discard its stream, so a microphone is never left
 * open by a stale async result.
 */
export class CaptureController {
  private generation = 0;
  private starting = false;
  private recorder: RecorderLike | null = null;
  private device: DeviceInfo | null = null;

  constructor(private readonly createRecorder: () => RecorderLike) {}

  get isStarting(): boolean { return this.starting; }
  get isActive(): boolean { return this.recorder !== null; }

  async start(opts: {
    acquire: () => Promise<{ stream: MediaStream; device: DeviceInfo }>;
    onLevel?: (level01: number) => void;
    onEnded?: () => void;
  }): Promise<StartResult> {
    if (this.starting) return { status: 'busy' };
    this.discardActive();
    const gen = ++this.generation;
    this.starting = true;
    try {
      let acquired: { stream: MediaStream; device: DeviceInfo };
      try {
        acquired = await opts.acquire();
      } catch (error) {
        return gen === this.generation ? { status: 'failed', error } : { status: 'cancelled' };
      }
      if (gen !== this.generation) {
        acquired.stream.getTracks().forEach((t) => t.stop());
        return { status: 'cancelled' };
      }
      const recorder = this.createRecorder();
      // Registered before start() so cancel() during the await reaches the recorder.
      this.recorder = recorder;
      this.device = acquired.device;
      try {
        await recorder.start(acquired.stream, opts.onLevel, opts.onEnded);
      } catch (error) {
        // PcmRecorder.start() already closed the context and stopped the tracks.
        if (this.recorder === recorder) this.clear();
        return gen === this.generation ? { status: 'failed', error } : { status: 'cancelled' };
      }
      return gen === this.generation && this.recorder === recorder ? { status: 'started' } : { status: 'cancelled' };
    } finally {
      if (gen === this.generation) this.starting = false;
    }
  }

  /** Stops and returns the PCM (null if nothing was recording). */
  async stop(): Promise<CapturedPcm | null> {
    const { recorder, device } = this;
    this.generation++;
    this.starting = false;
    this.clear();
    if (!recorder || !device) return null;
    try {
      return await recorder.stop(device);
    } catch {
      return null;
    }
  }

  /** Aborts any start in flight and discards (zeroes) the take in progress. */
  cancel(): void {
    this.generation++;
    this.starting = false;
    this.discardActive();
  }

  private discardActive(): void {
    const { recorder, device } = this;
    this.clear();
    if (recorder && device) {
      recorder.stop(device).then((c) => c.pcm.fill(0)).catch(() => undefined);
    }
  }

  private clear(): void {
    this.recorder = null;
    this.device = null;
  }
}
