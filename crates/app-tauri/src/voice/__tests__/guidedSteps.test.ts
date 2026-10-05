import { describe, it, expect } from 'vitest';
import { replacedSampleToDelete, stepAfterSelect, stepAfterSubmit } from '../hooks/guidedSteps';

const captured = { pcm: new Float32Array(4), sampleRate: 48_000 as const, durationSec: 3, peak: 0.4, rmsDbfs: -20, device: { label: 'Yeti', idHash: 'a'.repeat(64) } };

describe('replacedSampleToDelete', () => {
  it('deletes the old sample of the step once the new one is accepted', () => {
    expect(replacedSampleToDelete({ sampleId: 'old' }, { kind: 'done', quality: null }, 'new')).toBe('old');
  });
  it('keeps the old sample when the new one fails, for any failure', () => {
    expect(replacedSampleToDelete({ sampleId: 'old' }, { kind: 'show-error', code: 'ENROLL_TOO_QUIET' }, null)).toBeNull();
    expect(replacedSampleToDelete({ sampleId: 'old' }, { kind: 'show-budget-error', remainingSeconds: 1 }, null)).toBeNull();
  });
  it('deletes nothing on a first recording or when the step has no known sample id', () => {
    expect(replacedSampleToDelete(undefined, { kind: 'done', quality: null }, 'new')).toBeNull();
    expect(replacedSampleToDelete({ sampleId: null }, { kind: 'done', quality: null }, 'new')).toBeNull();
    expect(replacedSampleToDelete({}, { kind: 'done', quality: null }, 'new')).toBeNull();
  });
  it('never deletes the sample that was just accepted', () => {
    expect(replacedSampleToDelete({ sampleId: 'same' }, { kind: 'done', quality: null }, 'same')).toBeNull();
  });
  it('still deletes the old one when the service did not report the new id', () => {
    expect(replacedSampleToDelete({ sampleId: 'old' }, { kind: 'done', quality: null }, null)).toBe('old');
  });
});

describe('stepAfterSubmit', () => {
  it('records the accepted take with the service speech seconds and sample id', () => {
    const q = { peak: 0.4, rmsDbfs: -20, activeFraction: 0.7, speechSeconds: 2.5 };
    expect(stepAfterSubmit(undefined, { kind: 'done', quality: q }, 's1', captured)).toEqual({ duration: 2.5, sampleId: 's1' });
  });
  it('falls back to the captured duration without quality', () => {
    expect(stepAfterSubmit(undefined, { kind: 'done', quality: null }, 's1', captured)).toEqual({ duration: 3, sampleId: 's1' });
  });
  it('never keeps the PCM: it was zeroed after sending, so playing it would be silence (M1)', () => {
    const take = stepAfterSubmit(undefined, { kind: 'done', quality: null }, 's1', captured);
    expect(take && 'captured' in take).toBe(false);
  });
  it('keeps the previous take when the new one fails', () => {
    const prev = { duration: 4, sampleId: 'old' };
    expect(stepAfterSubmit(prev, { kind: 'show-error', code: 'ENROLL_FAILED' }, null, captured)).toBe(prev);
    expect(stepAfterSubmit(undefined, { kind: 'show-error', code: 'ENROLL_FAILED' }, null, captured)).toBeUndefined();
  });
});

describe('stepAfterSelect', () => {
  it('moves to the chosen step when idle', () => {
    expect(stepAfterSelect(1, 4, false)).toBe(4);
  });
  it('keeps the current step while recording', () => {
    expect(stepAfterSelect(2, 4, true)).toBe(2);
  });
  it('ignores steps outside 1..5', () => {
    expect(stepAfterSelect(3, 0, false)).toBe(3);
    expect(stepAfterSelect(3, 6, false)).toBe(3);
    expect(stepAfterSelect(3, 2.5, false)).toBe(3);
  });
});
