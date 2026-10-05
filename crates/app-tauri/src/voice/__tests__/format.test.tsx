import { describe, it, expect } from 'vitest';
import { renderToStaticMarkup as html } from 'react-dom/server';
import { formatDecimalLocale, formatSecondsLocale } from '../format';
import { VoiceBudgetMeter } from '../VoiceBudgetMeter';
import { VoiceSampleGallery } from '../VoiceSampleGallery';
import { EnrollmentJobStatus } from '../EnrollmentJobStatus';
import type { EnrollmentJob, EnrollmentLabels, ServiceSample } from '../enrollmentTypes';

const labels = new Proxy({} as Record<string, unknown>, {
  get: (_t, key) => (key === 'errors' ? { UNKNOWN: 'UNKNOWN' } : String(key)),
}) as unknown as EnrollmentLabels;

describe('formatSecondsLocale', () => {
  it('uses a decimal comma in pt-BR and a point in en-US, one decimal', () => {
    expect(formatSecondsLocale(84.25, 'pt-BR')).toBe('84,3');
    expect(formatSecondsLocale(84.25, 'en-US')).toBe('84.3');
    expect(formatSecondsLocale(90, 'pt-BR')).toBe('90,0');
    expect(formatSecondsLocale(-23.46, 'pt-BR')).toBe('-23,5');
  });
  it('treats any pt locale, in any case, as comma and anything else as point', () => {
    expect(formatSecondsLocale(1.5, 'PT')).toBe('1,5');
    expect(formatSecondsLocale(1.5, 'pt-PT')).toBe('1,5');
    expect(formatSecondsLocale(1.5, 'fr-FR')).toBe('1.5');
    expect(formatSecondsLocale(1.5, '')).toBe('1.5');
  });
  it('formats other decimal places the same way', () => {
    expect(formatDecimalLocale(0.5, 2, 'pt-BR')).toBe('0,50');
    expect(formatDecimalLocale(0.5, 2, 'en-US')).toBe('0.50');
  });
});

describe('components follow the language decimal separator', () => {
  const sample: ServiceSample = {
    id: 'a', name: 'A', timestamp: '1', speechSeconds: 4.24, deviceLabel: 'Yeti',
    usedInProfile: true, needsReenroll: false, otherMicrophone: false,
  };
  const done: EnrollmentJob = {
    jobId: 'j', state: 'done', stage: 'apply', errorCode: null, remainingSeconds: null, sampleId: 's', profileId: null,
    quality: { peak: 0.5, rmsDbfs: -23.46, activeFraction: 0.8, speechSeconds: 4.24 },
  };
  it('budget meter', () => {
    const m = html(<VoiceBudgetMeter budget={{ usedSeconds: 4.2, maxSeconds: 90, remainingSeconds: 85.8 }} labels={labels} lang="pt-BR" />);
    expect(m).toContain('4,2'); expect(m).toContain('90,0'); expect(m).toContain('85,8');
    expect(html(<VoiceBudgetMeter budget={{ usedSeconds: 4.2, maxSeconds: 90, remainingSeconds: 85.8 }} labels={labels} />)).toContain('85.8');
  });
  it('sample gallery', () => {
    const budget = { usedSeconds: 4.24, maxSeconds: 90, remainingSeconds: 85.76 };
    expect(html(<VoiceSampleGallery samples={[sample]} budget={budget} labels={labels} onDelete={() => {}} lang="pt-BR" />)).toContain('4,2 seconds');
    expect(html(<VoiceSampleGallery samples={[sample]} budget={budget} labels={labels} onDelete={() => {}} />)).toContain('4.2 seconds');
  });
  it('job quality', () => {
    const m = html(<EnrollmentJobStatus job={done} labels={labels} lang="pt-BR" />);
    expect(m).toContain('0,50'); expect(m).toContain('-23,5 dBFS'); expect(m).toContain('4,2 seconds');
    expect(html(<EnrollmentJobStatus job={done} labels={labels} />)).toContain('-23.5 dBFS');
  });
});
