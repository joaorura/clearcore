import { describe, it, expect } from 'vitest';
import { renderToStaticMarkup } from 'react-dom/server';
import { ptBR } from '../i18n/locales/pt-BR';
import { enUS } from '../i18n/locales/en-US';
import { I18nProvider } from '../i18n';
import { StudioDspCard, neuralEqCalibratedFrom } from '../StudioDspCard';

const DISHONEST = /ECAPA|Sincronizado|Synced|1\.8/;

describe('Studio DSP honesty (spec §9)', () => {
  it('no studioDsp text names the model, claims sync with the profile or a +1.8 dB gain', () => {
    for (const locale of [ptBR, enUS]) {
      for (const [key, text] of Object.entries(locale.studioDsp)) {
        expect(text, key).not.toMatch(DISHONEST);
      }
    }
  });

  it('calibration is true only when the service says so', () => {
    expect(neuralEqCalibratedFrom({ neural_eq_calibrated: true })).toBe(true);
    expect(neuralEqCalibratedFrom({ neural_eq_calibrated: false })).toBe(false);
    expect(neuralEqCalibratedFrom({ is_enrolled: true })).toBe(false);
    expect(neuralEqCalibratedFrom(undefined)).toBe(false);
    expect(neuralEqCalibratedFrom({ neural_eq_calibrated: 'true' })).toBe(false);
  });

  it('renders without service data showing no invented gains nor an affirmative calibration', () => {
    const html = renderToStaticMarkup(<I18nProvider><StudioDspCard /></I18nProvider>);
    expect(html).not.toContain('+1.8 dB');
    expect(html).not.toContain('-1.5 dB');
    expect(html).not.toContain(ptBR.studioDsp.neuralEqStatusCalibrated);
    expect(html).not.toMatch(/🟢 Calibrado/);
    expect(html).toContain(ptBR.studioDsp.neuralEqStatusPending);
  });
});
