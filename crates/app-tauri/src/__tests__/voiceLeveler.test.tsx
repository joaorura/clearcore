import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderToStaticMarkup } from 'react-dom/server';
import { ptBR } from '../i18n/locales/pt-BR';
import { enUS } from '../i18n/locales/en-US';
import { I18nProvider } from '../i18n';
import { StudioDspCard } from '../StudioDspCard';
import { getVoiceLeveler, setVoiceLeveler } from '../bridge';

describe('Voice Leveler (AGC) UI and Contract', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    if (typeof localStorage !== 'undefined') {
      localStorage.clear();
    }
  });

  it('provides all localized strings for Voice Auto-Leveler in pt-BR and en-US', () => {
    for (const locale of [ptBR, enUS]) {
      expect(locale.studioDsp.voiceLevelerSectionTitle).toBeDefined();
      expect(locale.studioDsp.voiceLevelerDesc).toBeDefined();
      expect(locale.studioDsp.voiceLevelerSliderLabel).toBeDefined();
      expect(locale.studioDsp.voiceLevelerOff).toBeDefined();
      expect(locale.studioDsp.voiceLevelerGentle).toBeDefined();
      expect(locale.studioDsp.voiceLevelerBalanced).toBeDefined();
      expect(locale.studioDsp.voiceLevelerFirm).toBeDefined();
      expect(locale.studioDsp.voiceLevelerFeedback).toBeDefined();
    }
  });

  it('renders the Voice Auto-Leveler section with slider and without a toggle button', () => {
    const html = renderToStaticMarkup(
      <I18nProvider>
        <StudioDspCard />
      </I18nProvider>
    );

    // Contains Voice Leveler section
    expect(html).toContain('voice-leveler-box');
    expect(html).toContain(ptBR.studioDsp.voiceLevelerSectionTitle);
    expect(html).toContain(ptBR.studioDsp.voiceLevelerSliderLabel);

    // Contains the slider input
    expect(html).toContain('id="voice-leveler-slider"');
    expect(html).toContain('type="range"');

    // Contains quick anchor buttons (0%, 30%, 50%, 100%)
    expect(html).toContain('voice-leveler-anchor-0');
    expect(html).toContain('voice-leveler-anchor-30');
    expect(html).toContain('voice-leveler-anchor-50');
    expect(html).toContain('voice-leveler-anchor-100');

    // No toggle switch in voice-leveler-box (only 1 slider as requested)
    const voiceBoxMatch = html.match(/<div class="voice-leveler-box"[^>]*>([\s\S]*?)<\/div>\s*<div class="neural-eq-box"/);
    expect(voiceBoxMatch).not.toBeNull();
    const voiceBoxContent = voiceBoxMatch ? voiceBoxMatch[1] : '';
    expect(voiceBoxContent).not.toContain('toggle-switch');
  });

  it('bridge helper getVoiceLeveler and setVoiceLeveler round-trip in browser environment', async () => {
    // Default fallback in browser is 0
    const initial = await getVoiceLeveler();
    expect(initial).toBe(0);

    // Set to 50%
    const res50 = await setVoiceLeveler(50);
    expect(res50.success).toBe(true);
    expect(res50.intensity).toBe(50);
    expect(await getVoiceLeveler()).toBe(50);

    // Clamps out-of-bounds values
    const resClamped = await setVoiceLeveler(120);
    expect(resClamped.intensity).toBe(100);
    expect(await getVoiceLeveler()).toBe(100);
  });
});
