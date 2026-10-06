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
    expect(html).not.toContain(ptBR.studioDsp.neuralEqCardCalibratedBadge);
  });

  it('calibration card copy explains active calibration honestly without fake gains or buzzwords', () => {
    for (const locale of [ptBR, enUS]) {
      expect(locale.studioDsp.neuralEqCardCalibratedBadge).toBeDefined();
      expect(locale.studioDsp.neuralEqCardActiveDesc).toBeDefined();
      expect(locale.studioDsp.neuralEqCardCalibratedBadge).not.toMatch(DISHONEST);
      expect(locale.studioDsp.neuralEqCardActiveDesc).not.toMatch(DISHONEST);
    }
  });

  it('renders neutral spectrum visual with all 5 frequency bands when not calibrated', () => {
    const html = renderToStaticMarkup(<I18nProvider><StudioDspCard /></I18nProvider>);
    expect(html).toContain('neural-eq-spectrum-visual');
    expect(html).toContain('80 Hz (Rumble / Corte de Subgraves)');
    expect(html).toContain('250 Hz (Corpo Vocal)');
    expect(html).toContain('1 kHz (Presença)');
    expect(html).toContain('3.5 kHz (Clareza)');
    expect(html).toContain('10 kHz (Ar / Brilho)');
    expect(html).toContain('0.0 dB');
    expect(html).toContain('bar-neutral');
    expect(html).not.toMatch(DISHONEST);
  });
});
