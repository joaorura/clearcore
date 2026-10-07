import { describe, it, expect } from 'vitest';
import { renderToStaticMarkup } from 'react-dom/server';
import { NoiseSuppressionSlider } from '../NoiseSuppressionSlider';
import { I18nProvider } from '../i18n';
import { ptBR } from '../i18n/locales/pt-BR';
import { enUS } from '../i18n/locales/en-US';
import { setFilterIntensity } from '../bridge';

describe('NoiseSuppressionSlider translations and anchors', () => {
  it('renders all 4 anchor labels correctly in both pt-BR and en-US', () => {
    expect(ptBR.filterIntensity.mild).toContain('Suave');
    expect(ptBR.filterIntensity.standard).toContain('Padrão');
    expect(ptBR.filterIntensity.aggressive).toContain('Agressivo');
    expect(ptBR.filterIntensity.maximum).toContain('Máximo');

    expect(enUS.filterIntensity.mild).toContain('Mild');
    expect(enUS.filterIntensity.standard).toContain('Standard');
    expect(enUS.filterIntensity.aggressive).toContain('Aggressive');
    expect(enUS.filterIntensity.maximum).toContain('Maximum');
  });

  it('renders slider and anchor buttons with default 50% intensity in HTML', () => {
    const html = renderToStaticMarkup(
      <I18nProvider>
        <NoiseSuppressionSlider />
      </I18nProvider>
    );

    expect(html).toContain('noise-suppression-intensity-block');
    expect(html).toContain('50%');
    expect(html).toContain('Padrão (50%)');
    expect(html).toContain('Suave (0%)');
    expect(html).toContain('Agressivo (75%)');
    expect(html).toContain('Máximo (100%)');
  });

  it('initializes from currentStatus if provided', () => {
    const html = renderToStaticMarkup(
      <I18nProvider>
        <NoiseSuppressionSlider
          currentStatus={{
            state: 'Running',
            is_terminal: false,
            can_restart: true,
            mode: 'Active',
            crash_count_15m: 0,
            total_crashes: 0,
            filter_intensity: 75,
          }}
        />
      </I18nProvider>
    );

    expect(html).toContain('75%');
    expect(html).toContain('Agressivo (75%)');
  });

  it('renders disabled attributes when disabled prop is true', () => {
    const html = renderToStaticMarkup(
      <I18nProvider>
        <NoiseSuppressionSlider disabled={true} />
      </I18nProvider>
    );

    expect(html).toContain('disabled=""');
  });
});

describe('bridge filter intensity functions', () => {
  it('setFilterIntensity returns success and filter_intensity', async () => {
    const res = await setFilterIntensity(75);
    expect(res.success).toBe(true);
    expect(res.filter_intensity).toBe(75);
  });

  it('setFilterIntensity clamps values to 0-100', async () => {
    const resOver = await setFilterIntensity(150);
    expect(resOver.filter_intensity).toBe(100);

    const resUnder = await setFilterIntensity(-10);
    expect(resUnder.filter_intensity).toBe(0);
  });
});
