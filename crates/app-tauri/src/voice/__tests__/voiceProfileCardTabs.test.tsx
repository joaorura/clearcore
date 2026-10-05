import { describe, it, expect, afterEach, vi } from 'vitest';
import { renderToStaticMarkup as html } from 'react-dom/server';
import { I18nProvider } from '../../i18n';
import { VoiceProfileCard } from '../../VoiceProfileCard';

const withStoredTab = (tab: string | null) =>
  vi.stubGlobal('localStorage', {
    getItem: (key: string) => (key === 'clearcore_voice_active_tab' ? tab : null),
    setItem: () => {},
  });

const render = () => html(<I18nProvider><VoiceProfileCard /></I18nProvider>);

afterEach(() => vi.unstubAllGlobals());

describe('VoiceProfileCard tabs', () => {
  it('shows the title above the four tabs and opens the first tab by default', () => {
    withStoredTab(null);
    const m = render();
    expect(m.indexOf('card-title')).toBeLessThan(m.indexOf('role="tablist"'));
    for (const label of ['Cadastro', 'Galeria', 'Chamadas', 'Perfil']) expect(m).toContain(label);
    expect(m).toContain('id="voice-profile-panel-enroll"');
  });
  it('reopens the stored tab', () => {
    withStoredTab('profile');
    const m = render();
    expect(m).toContain('id="voice-profile-panel-profile"');
    expect(m).toContain('Modelo de enrollment de desenvolvimento, ainda não aprovado');
  });
  it('ignores an invalid stored value', () => {
    withStoredTab('samples');
    expect(render()).toContain('id="voice-profile-panel-enroll"');
  });
  it('shows the development model notice exactly once, above the tabs, on every tab', () => {
    const notice = 'Modelo de enrollment de desenvolvimento, ainda não aprovado';
    for (const tab of ['enroll', 'gallery', 'calls', 'profile']) {
      withStoredTab(tab);
      const m = render();
      expect(m.split(notice).length - 1, tab).toBe(1);
      expect(m.indexOf(notice), tab).toBeLessThan(m.indexOf('role="tablist"'));
      expect(m, tab).toContain(`id="voice-profile-panel-${tab}"`);
    }
  });
});
