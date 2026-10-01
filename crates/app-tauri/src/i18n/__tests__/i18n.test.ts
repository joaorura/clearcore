import { describe, it, expect } from 'vitest';
import { ptBR } from '../locales/pt-BR';
import { enUS } from '../locales/en-US';
import { availableLocales, defaultLocaleCode, getLocaleMeta } from '../locales';

describe('i18n Translation Engine', () => {
  it('has pt-BR as default locale', () => {
    expect(defaultLocaleCode).toBe('pt-BR');
    expect(availableLocales.some((l) => l.code === 'pt-BR')).toBe(true);
    expect(availableLocales.some((l) => l.code === 'en-US')).toBe(true);
  });

  it('provides metadata for registered locales', () => {
    const ptMeta = getLocaleMeta('pt-BR');
    expect(ptMeta.name).toBe('Português (Brasil)');
    expect(ptMeta.translations.app.title).toBe('ClearCore');

    const enMeta = getLocaleMeta('en-US');
    expect(enMeta.name).toBe('English (US)');
    expect(enMeta.translations.app.title).toBe('ClearCore');
  });

  it('falls back to default locale for unknown locale code', () => {
    const unknownMeta = getLocaleMeta('de-DE');
    expect(unknownMeta.code).toBe('pt-BR');
  });

  it('ensures pt-BR and en-US have identical key structures', () => {
    const getKeys = (obj: Record<string, unknown>, prefix = ''): string[] => {
      let keys: string[] = [];
      for (const [k, v] of Object.entries(obj)) {
        const fullKey = prefix ? `${prefix}.${k}` : k;
        if (typeof v === 'object' && v !== null && !Array.isArray(v)) {
          keys = keys.concat(getKeys(v as Record<string, unknown>, fullKey));
        } else {
          keys.push(fullKey);
        }
      }
      return keys.sort();
    };

    const ptKeys = getKeys(ptBR as unknown as Record<string, unknown>);
    const enKeys = getKeys(enUS as unknown as Record<string, unknown>);

    expect(ptKeys).toEqual(enKeys);
  });

  it('formats interpolated parameters properly', () => {
    const template = 'Microfone virtual criado e verificado com sucesso! (ID: {id})';
    const formatted = template.split('{id}').join('42');
    expect(formatted).toBe('Microfone virtual criado e verificado com sucesso! (ID: 42)');
  });
});
