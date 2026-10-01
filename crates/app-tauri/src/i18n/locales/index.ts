import { LocaleMeta } from '../types';
import { ptBR } from './pt-BR';
import { enUS } from './en-US';

export const availableLocales: LocaleMeta[] = [
  {
    code: 'pt-BR',
    name: 'Português (Brasil)',
    flag: '🇧🇷',
    translations: ptBR,
  },
  {
    code: 'en-US',
    name: 'English (US)',
    flag: '🇺🇸',
    translations: enUS,
  },
];

export const defaultLocaleCode = 'pt-BR';

export function getLocaleMeta(code: string): LocaleMeta {
  const found = availableLocales.find((l) => l.code === code);
  return found || availableLocales[0];
}
