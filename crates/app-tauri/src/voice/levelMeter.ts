/**
 * Live level meter math (no React, no timers: time is always passed in, so it is testable).
 *
 * The meter shows dBFS, not linear peak: normal speech peaks around 0.1 to 0.18 (-20 to -15 dBFS),
 * which a linear bar draws at only 10 to 18 %. -60..0 dBFS maps to 0..100 %.
 */

export const METER_FLOOR_DBFS = -60;
/** Fast attack, peak-hold, then a smooth fall. */
export const METER_HOLD_MS = 300;
export const METER_DECAY_DB_PER_SEC = 40;
/** The bar is refreshed at ~30 Hz; peaks between refreshes are accumulated, never dropped. */
export const METER_MIN_INTERVAL_MS = 33;

export function linearToDbfs(linear: number): number {
  return linear > 0 ? Math.max(-120, 20 * Math.log10(linear)) : -120;
}

/** -60..0 dBFS to 0..100 (integer), clamped. */
export function dbfsToPercent(dbfs: number): number {
  if (!Number.isFinite(dbfs)) return dbfs > 0 ? 100 : 0;
  const pct = ((dbfs - METER_FLOOR_DBFS) / -METER_FLOOR_DBFS) * 100;
  return Math.round(Math.min(100, Math.max(0, pct)));
}

export function peakToPercent(peak: number): number {
  return dbfsToPercent(linearToDbfs(peak));
}

/** Attack instantly, hold the peak for `holdMs`, then fall `decayDbPerSec`; emits at most every `minIntervalMs`. */
export class LevelSmoother {
  private currentDb = -120;
  private pendingPeak = 0;
  private lastEmit: number | null = null;
  private holdUntil = 0;

  constructor(
    private readonly opts: { holdMs?: number; decayDbPerSec?: number; minIntervalMs?: number } = {},
  ) {}

  /** Feeds one chunk's peak; returns the percent to display when an update is due, otherwise null. */
  push(peak: number, nowMs: number): number | null {
    const { holdMs = METER_HOLD_MS, decayDbPerSec = METER_DECAY_DB_PER_SEC, minIntervalMs = METER_MIN_INTERVAL_MS } = this.opts;
    this.pendingPeak = Math.max(this.pendingPeak, Math.abs(peak));
    if (this.lastEmit !== null && nowMs - this.lastEmit < minIntervalMs) return null;
    const db = linearToDbfs(Math.min(1, this.pendingPeak));
    this.pendingPeak = 0;
    if (db >= this.currentDb) {
      this.currentDb = db;
      this.holdUntil = nowMs + holdMs;
    } else {
      const decayFrom = Math.max(this.holdUntil, this.lastEmit ?? nowMs);
      const decayMs = Math.max(0, nowMs - decayFrom);
      this.currentDb = Math.max(db, this.currentDb - (decayDbPerSec * decayMs) / 1000);
    }
    this.lastEmit = nowMs;
    return dbfsToPercent(this.currentDb);
  }

  reset(): void {
    this.currentDb = -120;
    this.pendingPeak = 0;
    this.lastEmit = null;
    this.holdUntil = 0;
  }
}

/** The daemon rejects a take whose active speech is below -40 dBFS; warn a little before that. */
export const LOW_SPEECH_DBFS = -36;
/** Windows below this are pauses, not speech: they neither trigger nor clear the hint. */
export const SPEECH_FLOOR_DBFS = -50;
export const LOW_SPEECH_WINDOW_MS = 100;
export const LOW_SPEECH_MIN_MS = 1000;
/** Speech windows considered (1.5 s): a single louder word within it clears the hint. */
const LOW_SPEECH_HISTORY_WINDOWS = 15;
const LOW_SPEECH_CLEAR_DBFS = -34;

/**
 * Tells when the speech heard so far is too quiet: the RMS of the last ~1.5 s of speech windows
 * (100 ms each, pauses skipped) stays below -36 dBFS for more than ~1 s. Sample-clocked, not wall-clocked.
 */
export class LowSpeechDetector {
  private sumSq = 0;
  private samples = 0;
  private readonly ring: number[] = []; // mean-square of the recent speech windows
  private hint = false;
  private readonly windowSamples: number;

  constructor(sampleRate = 48000) {
    this.windowSamples = Math.round((sampleRate * LOW_SPEECH_WINDOW_MS) / 1000);
  }

  /** Feeds one chunk (its RMS and length); returns true while the hint should be shown. */
  push(chunkRms: number, frames: number): boolean {
    this.sumSq += chunkRms * chunkRms * frames;
    this.samples += frames;
    while (this.samples >= this.windowSamples) {
      const ms = this.sumSq / this.samples;
      this.sumSq = 0;
      this.samples = 0;
      this.closeWindow(ms);
    }
    return this.hint;
  }

  private closeWindow(meanSquare: number): void {
    const db = linearToDbfs(Math.sqrt(meanSquare));
    if (db <= SPEECH_FLOOR_DBFS) return; // pause
    this.ring.push(meanSquare);
    if (this.ring.length > LOW_SPEECH_HISTORY_WINDOWS) this.ring.shift();
    if (this.ring.length * LOW_SPEECH_WINDOW_MS <= LOW_SPEECH_MIN_MS) return;
    const avg = this.ring.reduce((a, b) => a + b, 0) / this.ring.length;
    const avgDb = linearToDbfs(Math.sqrt(avg));
    if (avgDb < LOW_SPEECH_DBFS) this.hint = true;
    else if (avgDb > LOW_SPEECH_CLEAR_DBFS) this.hint = false;
  }

  reset(): void {
    this.sumSq = 0;
    this.samples = 0;
    this.ring.length = 0;
    this.hint = false;
  }
}
