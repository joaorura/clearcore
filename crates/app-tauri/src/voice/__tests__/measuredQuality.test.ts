import { describe, it, expect } from 'vitest';
import { measuredQualityText, nextStepAfterJob } from '../hooks/voiceProfileLogic';
import { ptBR } from '../../i18n/locales/pt-BR';
import { enUS } from '../../i18n/locales/en-US';
import type { EnrollmentJob } from '../enrollmentTypes';

const t = (path: string, params?: Record<string, string | number>) =>
  params ? `${path}(${Object.entries(params).map(([k, v]) => `${k}=${v}`).join(';')})` : path;
const q = { peak: 0.998, rmsDbfs: -52.34, activeFraction: 0.1, speechSeconds: 1.26 };
const failed = (over: Partial<EnrollmentJob>): EnrollmentJob => ({
  jobId: 'sample-job-1', state: 'failed', stage: 'trim', errorCode: 'ENROLL_CLIPPING', remainingSeconds: null,
  sampleId: null, profileId: null, quality: null, ...over,
});

describe('measured values on quality errors (spec §9)', () => {
  it('a failed job carries its measured quality when the service sent one', () => {
    expect(nextStepAfterJob(failed({ quality: q }))).toEqual({ kind: 'show-error', code: 'ENROLL_CLIPPING', quality: q });
    expect(nextStepAfterJob(failed({}))).toEqual({ kind: 'show-error', code: 'ENROLL_CLIPPING' });
  });
  it('clipping shows the peak, too quiet the level, too little speech the speech seconds (localized)', () => {
    expect(measuredQualityText('ENROLL_CLIPPING', q, t, 'pt-BR')).toBe('voiceProfile.measuredPeak(peak=1,00)');
    expect(measuredQualityText('ENROLL_TOO_QUIET', q, t, 'pt-BR')).toBe('voiceProfile.measuredLevel(db=-52,3)');
    expect(measuredQualityText('ENROLL_TOO_LITTLE_SPEECH', q, t, 'en-US')).toBe('voiceProfile.measuredSpeech(sec=1.3)');
  });
  it('nothing without quality or for other codes', () => {
    expect(measuredQualityText('ENROLL_CLIPPING', null, t, 'pt-BR')).toBeNull();
    expect(measuredQualityText('ENROLL_FAILED', q, t, 'pt-BR')).toBeNull();
  });
  it('the labels exist in both locales', () => {
    for (const l of [ptBR, enUS]) {
      expect(l.voiceProfile.measuredPeak).toContain('{peak}');
      expect(l.voiceProfile.measuredLevel).toContain('{db}');
      expect(l.voiceProfile.measuredSpeech).toContain('{sec}');
    }
  });
});
