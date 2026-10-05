import { describe, it, expect } from 'vitest';
import { stepAfterSubmit } from '../hooks/guidedSteps';

const captured = { pcm: new Float32Array(4), sampleRate: 48_000 as const, durationSec: 3, peak: 0.4, rmsDbfs: -20, device: { label: 'Yeti', idHash: 'a'.repeat(64) } };

describe('stepAfterSubmit', () => {
  it('records the accepted take with the service speech seconds and sample id', () => {
    const q = { peak: 0.4, rmsDbfs: -20, activeFraction: 0.7, speechSeconds: 2.5 };
    expect(stepAfterSubmit(undefined, { kind: 'done', quality: q }, 's1', captured)).toEqual({ duration: 2.5, captured, sampleId: 's1' });
  });
  it('falls back to the captured duration without quality', () => {
    expect(stepAfterSubmit(undefined, { kind: 'done', quality: null }, 's1', captured)).toEqual({ duration: 3, captured, sampleId: 's1' });
  });
  it('keeps the previous take when the new one fails', () => {
    const prev = { duration: 4, sampleId: 'old' };
    expect(stepAfterSubmit(prev, { kind: 'show-error', code: 'ENROLL_FAILED' }, null, captured)).toBe(prev);
    expect(stepAfterSubmit(undefined, { kind: 'show-error', code: 'ENROLL_FAILED' }, null, captured)).toBeUndefined();
  });
});
