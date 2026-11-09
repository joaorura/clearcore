import { describe, it, expect } from 'vitest';
import {
  LevelSmoother, LowSpeechDetector, dbfsToPercent, linearToDbfs, peakToPercent,
} from '../levelMeter';

const db = (x: number) => 10 ** (x / 20);

describe('dBFS mapping', () => {
  it('maps -60..0 dBFS to 0..100 %, clamped', () => {
    expect(dbfsToPercent(-60)).toBe(0);
    expect(dbfsToPercent(-30)).toBe(50);
    expect(dbfsToPercent(0)).toBe(100);
    expect(dbfsToPercent(-90)).toBe(0);
    expect(dbfsToPercent(6)).toBe(100);
  });
  it('shows normal speech (peak 0.1 to 0.18) well above the old 10 to 18 %', () => {
    expect(peakToPercent(0.1)).toBe(67);
    expect(peakToPercent(0.18)).toBe(75);
    expect(peakToPercent(0)).toBe(0);
    expect(peakToPercent(1)).toBe(100);
  });
  it('linearToDbfs floors silence', () => {
    expect(linearToDbfs(0)).toBe(-120);
    expect(linearToDbfs(1)).toBeCloseTo(0, 6);
  });
});

describe('LevelSmoother', () => {
  it('attacks instantly', () => {
    const s = new LevelSmoother();
    expect(s.push(0, 0)).toBe(0);
    expect(s.push(db(-20), 40)).toBe(67);
  });
  it('limits updates to ~30 Hz but never loses a peak between them', () => {
    const s = new LevelSmoother();
    expect(s.push(0.001, 0)).not.toBeNull();
    expect(s.push(0.5, 5)).toBeNull(); // accumulated
    expect(s.push(0.001, 10)).toBeNull();
    const shown = s.push(0.001, 40);
    expect(shown).toBe(peakToPercent(0.5));
  });
  it('holds the peak ~300 ms and then falls smoothly', () => {
    const s = new LevelSmoother();
    s.push(db(-20), 0);
    expect(s.push(0, 100)).toBe(67); // still holding
    expect(s.push(0, 290)).toBe(67);
    const a = s.push(0, 400)!; // 100 ms after the hold: ~4 dB lower
    expect(a).toBeLessThan(67);
    expect(a).toBeGreaterThan(55);
    const b = s.push(0, 600)!;
    expect(b).toBeLessThan(a);
    expect(s.push(0, 3000)).toBe(0);
  });
  it('reset clears the bar', () => {
    const s = new LevelSmoother();
    s.push(0.5, 0);
    s.reset();
    expect(s.push(0, 0)).toBe(0);
  });
});

describe('LowSpeechDetector', () => {
  const chunk = 128;
  const feed = (d: LowSpeechDetector, rmsDb: number, seconds: number) => {
    let hint = false;
    const n = Math.round((seconds * 48000) / chunk);
    for (let i = 0; i < n; i++) hint = d.push(db(rmsDb), chunk);
    return hint;
  };
  it('stays quiet at normal speech level', () => {
    const d = new LowSpeechDetector();
    expect(feed(d, -25, 3)).toBe(false);
  });
  it('fires after more than 1 s of speech below -36 dBFS', () => {
    const d = new LowSpeechDetector();
    expect(feed(d, -42, 0.8)).toBe(false);
    expect(feed(d, -42, 0.5)).toBe(true);
  });
  it('ignores pauses (below the speech floor): they neither trigger nor clear it', () => {
    const d = new LowSpeechDetector();
    expect(feed(d, -70, 5)).toBe(false);
    feed(d, -42, 1.5);
    expect(feed(d, -70, 2)).toBe(true);
  });
  it('clears once the voice gets loud enough', () => {
    const d = new LowSpeechDetector();
    feed(d, -42, 1.5);
    expect(feed(d, -20, 2)).toBe(false);
  });
  it('reset drops the hint', () => {
    const d = new LowSpeechDetector();
    feed(d, -42, 2);
    d.reset();
    expect(feed(d, -42, 0.5)).toBe(false);
  });
});
