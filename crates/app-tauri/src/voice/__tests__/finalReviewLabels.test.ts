import { describe, it, expect } from 'vitest';
import { ptBR } from '../../i18n/locales/pt-BR';
import { enUS } from '../../i18n/locales/en-US';

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
