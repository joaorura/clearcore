import { describe, it, expect } from 'vitest';
import {
  initialStepFromCompleted,
  matchSampleToStep,
  replacedSampleToDelete,
  stepAfterSelect,
  stepAfterSubmit,
  syncCompletedStepsFromSamples,
} from '../hooks/guidedSteps';

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
  it('keeps the current step while recording or submitting', () => {
    expect(stepAfterSelect(2, 4, true)).toBe(2);
  });
  it('ignores steps outside 1..5', () => {
    expect(stepAfterSelect(3, 0, false)).toBe(3);
    expect(stepAfterSelect(3, 6, false)).toBe(3);
    expect(stepAfterSelect(3, 2.5, false)).toBe(3);
  });
});

describe('matchSampleToStep', () => {
  it('matches category names in pt-BR and en-US', () => {
    expect(matchSampleToStep({ name: 'Início de Reunião' }, 1)).toBe(true);
    expect(matchSampleToStep({ name: 'Meeting Kickoff' }, 1)).toBe(true);
    expect(matchSampleToStep({ name: 'Rotina Matinal' }, 2)).toBe(true);
    expect(matchSampleToStep({ name: 'Morning Routine' }, 2)).toBe(true);
    expect(matchSampleToStep({ name: 'Foco de Trabalho' }, 3)).toBe(true);
    expect(matchSampleToStep({ name: 'Work Focus' }, 3)).toBe(true);
    expect(matchSampleToStep({ name: 'Espaço de Trabalho' }, 4)).toBe(true);
    expect(matchSampleToStep({ name: 'Workspace' }, 4)).toBe(true);
    expect(matchSampleToStep({ name: 'Lazer / Descontração' }, 5)).toBe(true);
    expect(matchSampleToStep({ name: 'Leisure & Downtime' }, 5)).toBe(true);
  });

  it('matches case and diacritics insensitively', () => {
    expect(matchSampleToStep({ name: 'inicio de reuniao' }, 1)).toBe(true);
    expect(matchSampleToStep({ name: 'rotina MATINAL' }, 2)).toBe(true);
    expect(matchSampleToStep({ name: 'espaco de trabalho' }, 4)).toBe(true);
    expect(matchSampleToStep({ name: 'lazer e descontracao' }, 5)).toBe(true);
  });

  it('matches explicit step numbering in sample name', () => {
    expect(matchSampleToStep({ name: 'Etapa 1' }, 1)).toBe(true);
    expect(matchSampleToStep({ name: 'Passo 2' }, 2)).toBe(true);
    expect(matchSampleToStep({ name: 'Step 3' }, 3)).toBe(true);
    expect(matchSampleToStep({ name: 'Amostra 4' }, 4)).toBe(true);
    expect(matchSampleToStep({ name: 'Sample 5' }, 5)).toBe(true);
  });

  it('does not match unrelated sample names', () => {
    expect(matchSampleToStep({ name: 'Amostra Livre' }, 1)).toBe(false);
    expect(matchSampleToStep({ name: 'Gravação da Chamada' }, 2)).toBe(false);
    expect(matchSampleToStep({ name: '' }, 1)).toBe(false);
  });
});

describe('syncCompletedStepsFromSamples', () => {
  const makeSample = (id: string, name: string, speechSeconds: number, timestamp: string, otherMicrophone = false) => ({
    id, name, speechSeconds, timestamp, otherMicrophone,
    deviceLabel: 'Mic', usedInProfile: true, needsReenroll: false,
  });

  it('populates completedSteps from daemon gallery samples', () => {
    const samples = [
      makeSample('s1', 'Início de Reunião', 5.2, '1000'),
      makeSample('s2', 'Rotina Matinal', 4.8, '2000'),
    ];
    const synced = syncCompletedStepsFromSamples(samples, {});
    expect(synced[1]).toEqual({ duration: 5.2, sampleId: 's1' });
    expect(synced[2]).toEqual({ duration: 4.8, sampleId: 's2' });
    expect(synced[3]).toBeUndefined();
  });

  it('preserves existing step take when still present in service', () => {
    const samples = [makeSample('s1', 'Início de Reunião', 5.2, '1000')];
    const prev = { 1: { duration: 5.0, sampleId: 's1' } };
    const synced = syncCompletedStepsFromSamples(samples, prev);
    expect(synced[1]).toEqual({ duration: 5.0, sampleId: 's1' });
  });

  it('clears step take when the sample was deleted from the service', () => {
    const samples = [makeSample('s2', 'Rotina Matinal', 4.8, '2000')];
    const prev = { 1: { duration: 5.0, sampleId: 'deleted-s1' } };
    const synced = syncCompletedStepsFromSamples(samples, prev);
    expect(synced[1]).toBeUndefined();
    expect(synced[2]).toEqual({ duration: 4.8, sampleId: 's2' });
  });

  it('prefers current microphone samples over otherMicrophone samples', () => {
    const samples = [
      makeSample('s-old-mic', 'Início de Reunião', 4.0, '2000', true),
      makeSample('s-curr-mic', 'Início de Reunião', 6.0, '1000', false),
    ];
    const synced = syncCompletedStepsFromSamples(samples, {});
    expect(synced[1]).toEqual({ duration: 6.0, sampleId: 's-curr-mic' });
  });
});

describe('initialStepFromCompleted', () => {
  it('returns 1 when no steps are completed', () => {
    expect(initialStepFromCompleted({})).toBe(1);
  });
  it('returns next incomplete step in order', () => {
    expect(initialStepFromCompleted({ 1: { duration: 3 } })).toBe(2);
    expect(initialStepFromCompleted({ 1: { duration: 3 }, 2: { duration: 4 } })).toBe(3);
    expect(initialStepFromCompleted({ 1: { duration: 3 }, 2: { duration: 4 }, 3: { duration: 5 } })).toBe(4);
  });
  it('returns 5 when all steps are completed', () => {
    const all = { 1: { duration: 3 }, 2: { duration: 3 }, 3: { duration: 3 }, 4: { duration: 3 }, 5: { duration: 3 } };
    expect(initialStepFromCompleted(all)).toBe(5);
  });
});

