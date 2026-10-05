import { CAPTURE_SAMPLE_RATE, MAX_RECORD_SECONDS } from './enrollmentTypes';
import type { CapturedPcm, DeviceInfo } from './enrollmentTypes';
import { PCM_WORKLET_SOURCE } from './pcmWorklet';

/*
 * Plan B (NOT implemented): if loading the AudioWorklet from a Blob URL is blocked in the
 * packaged Electron context (file:// origin / CSP), replace the worklet with a
 * ScriptProcessorNode (deprecated but available): createScriptProcessor(4096, 1, 1) and copy
 * event.inputBuffer.getChannelData(0) in onaudioprocess, feeding the same chunk list.
 */

export function concatChunks(chunks: Float32Array[]): Float32Array {
  let total = 0;
  for (const c of chunks) total += c.length;
  const out = new Float32Array(total);
  let offset = 0;
  for (const c of chunks) {
    out.set(c, offset);
    offset += c.length;
  }
  return out;
}

export function measurePcm(pcm: Float32Array): { peak: number; rmsDbfs: number } {
  let peak = 0;
  let sumSq = 0;
  for (let i = 0; i < pcm.length; i++) {
    const v = pcm[i];
    const a = Math.abs(v);
    if (a > peak) peak = a;
    sumSq += v * v;
  }
  const rms = pcm.length > 0 ? Math.sqrt(sumSq / pcm.length) : 0;
  return { peak, rmsDbfs: rms > 0 ? Math.max(-120, 20 * Math.log10(rms)) : -120 };
}

export class PcmRecorder {
  private readonly maxSeconds: number;
  private chunks: Float32Array[] = [];
  private samples = 0;
  private context: AudioContext | null = null;
  private source: MediaStreamAudioSourceNode | null = null;
  private node: AudioWorkletNode | null = null;
  private stream: MediaStream | null = null;
  private cancelled = false;
  private tornDown = false;
  /** True once the duration guard dropped samples beyond maxSeconds. */
  truncated = false;

  constructor(opts?: { maxSeconds?: number }) {
    this.maxSeconds = opts?.maxSeconds ?? MAX_RECORD_SECONDS;
  }

  /** Closes the context, disconnects the nodes and stops every track. Idempotent. */
  private async teardown(): Promise<number> {
    const context = this.context;
    const rate = context?.sampleRate ?? CAPTURE_SAMPLE_RATE;
    try { this.source?.disconnect(); } catch { /* ignore */ }
    if (this.node) this.node.port.onmessage = null;
    try { this.node?.disconnect(); } catch { /* ignore */ }
    this.stream?.getTracks().forEach((t) => {
      try { t.stop(); } catch { /* ignore */ }
    });
    this.context = this.source = this.node = this.stream = null;
    this.tornDown = true;
    if (context && context.state !== 'closed') {
      try { await context.close(); } catch { /* ignore */ }
    }
    return rate;
  }

  /**
   * Starts capturing. Owns the stream: on any failure (or if stop() is called meanwhile) the
   * context is closed and every track stopped, so the caller's microphone safety does not depend
   * on the caller. `onEnded` fires when the microphone track ends (device unplugged).
   */
  async start(stream: MediaStream, onLevel?: (level01: number) => void, onEnded?: () => void): Promise<void> {
    this.chunks = [];
    this.samples = 0;
    this.truncated = false;
    this.cancelled = false;
    this.tornDown = false;
    this.stream = stream;
    try {
      const context = new AudioContext({ sampleRate: CAPTURE_SAMPLE_RATE });
      this.context = context;
      const url = URL.createObjectURL(new Blob([PCM_WORKLET_SOURCE], { type: 'application/javascript' }));
      try {
        await context.audioWorklet.addModule(url);
      } finally {
        URL.revokeObjectURL(url);
      }
      if (this.cancelled) return void (await this.teardown());
      const node = new AudioWorkletNode(context, 'pcm-capture', { numberOfInputs: 1, numberOfOutputs: 0 });
      this.node = node;
      const maxSamples = Math.floor(this.maxSeconds * context.sampleRate);
      node.port.onmessage = (ev: MessageEvent<Float32Array>) => {
        const chunk = ev.data;
        if (this.samples >= maxSamples) {
          this.truncated = true;
          return;
        }
        const room = maxSamples - this.samples;
        if (chunk.length > room) this.truncated = true;
        const kept = chunk.length > room ? chunk.subarray(0, room) : chunk;
        this.chunks.push(kept);
        this.samples += kept.length;
        if (onLevel) {
          let peak = 0;
          for (let i = 0; i < chunk.length; i++) peak = Math.max(peak, Math.abs(chunk[i]));
          onLevel(Math.min(1, peak));
        }
      };
      this.source = context.createMediaStreamSource(stream);
      this.source.connect(node);
      if (onEnded) {
        for (const track of stream.getTracks()) track.addEventListener?.('ended', onEnded);
      }
      await context.resume();
      if (this.cancelled) await this.teardown();
    } catch (err) {
      await this.teardown();
      throw err;
    }
  }

  async stop(device: DeviceInfo): Promise<CapturedPcm> {
    this.cancelled = true;
    const rate = this.tornDown ? CAPTURE_SAMPLE_RATE : await this.teardown();
    const pcm = concatChunks(this.chunks);
    this.chunks = [];
    this.samples = 0;
    const { peak, rmsDbfs } = measurePcm(pcm);
    return {
      pcm,
      sampleRate: rate as typeof CAPTURE_SAMPLE_RATE,
      durationSec: pcm.length / rate,
      peak,
      rmsDbfs,
      device,
    };
  }
}
