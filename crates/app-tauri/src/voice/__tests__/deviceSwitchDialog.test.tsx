import { it, expect } from 'vitest';
import { renderToStaticMarkup as html } from 'react-dom/server';
import { ptBR } from '../../i18n/locales/pt-BR';
import { enUS } from '../../i18n/locales/en-US';
import { DeviceSwitchDialog } from '../panels/DeviceSwitchDialog';

const tOf = (dict: Record<string, string>) => (path: string, params?: Record<string, string | number>) => {
  let s = dict[path.replace('voiceProfile.', '')] ?? `MISSING:${path}`;
  for (const [k, v] of Object.entries(params ?? {})) s = s.split(`{${k}}`).join(String(v));
  return s;
};

it('warns that the earlier samples stop being used, in both locales, with both microphone names', () => {
  const pt = html(<DeviceSwitchDialog t={tOf(ptBR.voiceProfile as unknown as Record<string, string>)} info={{ newLabel: 'Yeti', oldLabel: 'Headset' }} onAnswer={() => {}} />);
  expect(pt).toContain('deixam de ser usadas no perfil');
  expect(pt).toContain('Yeti'); expect(pt).toContain('Headset');
  expect(pt).not.toContain('MISSING');
  const en = html(<DeviceSwitchDialog t={tOf(enUS.voiceProfile as unknown as Record<string, string>)} info={{ newLabel: 'Yeti', oldLabel: 'Headset' }} onAnswer={() => {}} />);
  expect(en).toContain('stop being used in the profile');
  expect(en).not.toContain('MISSING');
});
