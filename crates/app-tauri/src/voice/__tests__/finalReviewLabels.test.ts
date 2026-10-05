import { describe, it, expect } from 'vitest';
import { ptBR } from '../../i18n/locales/pt-BR';
import { enUS } from '../../i18n/locales/en-US';
import { renderToStaticMarkup as html } from 'react-dom/server';
import { createElement } from 'react';
import { enrollmentErrorCode } from '../enrollmentErrors';
import { buildEnrollmentLabels } from '../hooks/voiceProfileLogic';
import { GalleryPanel, type GalleryPanelProps } from '../panels/GalleryPanel';
import type { SampleList } from '../enrollmentTypes';

describe('ENROLL_MODEL_NOT_CONFIGURED label (I5)', () => {
  it('covers both the enrollment model and a missing isolation model/denoiser', () => {
    const pt = ptBR.voiceProfile.enrollment.errors.ENROLL_MODEL_NOT_CONFIGURED;
    const en = enUS.voiceProfile.enrollment.errors.ENROLL_MODEL_NOT_CONFIGURED;
    expect(pt).toMatch(/cadastro/i);
    expect(pt).toMatch(/isolamento|denoiser/i);
    expect(en).toMatch(/enrollment/i);
    expect(en).toMatch(/isolation|denoiser/i);
  });
});

describe('approve call take label (M4)', () => {
  it('says the take goes to the Gallery, not straight into the profile', () => {
    expect(ptBR.voiceProfile.approveTake).toMatch(/Aprovar e mover para a Galeria/);
    expect(enUS.voiceProfile.approveTake).toMatch(/Approve and move to the Gallery/);
    for (const label of [ptBR.voiceProfile.approveTake, enUS.voiceProfile.approveTake]) {
      expect(label).not.toMatch(/perfil|profile/i);
    }
  });
});

describe('older service (I3)', () => {
  const t = (k: string) => k;
  const labels = buildEnrollmentLabels(t);
  const list = (over: Partial<SampleList>): SampleList => ({ samples: [], budget: { usedSeconds: 0, maxSeconds: 90, remainingSeconds: 90 }, ...over });
  const props = (sampleList: SampleList): GalleryPanelProps => ({
    t, locale: 'pt-BR', labels, sampleList, samplesLoadFailed: false, deletingId: null, budgetError: null, feedback: null,
    onDelete: () => {}, onAddSample: () => {},
  });
  it('SERVICE_OUTDATED is a known local code with a label in both locales', () => {
    expect(enrollmentErrorCode({ errorCode: 'SERVICE_OUTDATED' })).toBe('SERVICE_OUTDATED');
    expect(labels.errors.SERVICE_OUTDATED).toBe('voiceProfile.enrollment.errors.SERVICE_OUTDATED');
    expect(ptBR.voiceProfile.enrollment.errors.SERVICE_OUTDATED).toMatch(/desatualizado.*reinicie o ClearCore/i);
    expect(enUS.voiceProfile.enrollment.errors.SERVICE_OUTDATED).toMatch(/out of date.*restart ClearCore/i);
  });
  it('the gallery shows the outdated-service notice instead of a "0 remaining" meter', () => {
    const m = html(createElement(GalleryPanel, props(list({ serviceOutdated: true, budget: { usedSeconds: 0, maxSeconds: 90, remainingSeconds: 0 } }))));
    expect(m).toContain('voiceProfile.enrollment.errors.SERVICE_OUTDATED');
    expect(m).not.toContain('voice-budget-meter');
    const ok = html(createElement(GalleryPanel, props(list({ serviceOutdated: false }))));
    expect(ok).toContain('voice-budget-meter');
    expect(ok).not.toContain('SERVICE_OUTDATED');
  });
});
