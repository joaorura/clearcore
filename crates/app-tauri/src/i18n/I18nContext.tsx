import React, { createContext, useContext, useState, useMemo, ReactNode } from 'react';
import { LocaleCode, LocaleMeta } from './types';
import { availableLocales, defaultLocaleCode, getLocaleMeta } from './locales';

interface I18nContextType {
  locale: LocaleCode;
  setLocale: (code: LocaleCode) => void;
  currentMeta: LocaleMeta;
  availableLocales: LocaleMeta[];
  t: (path: string, params?: Record<string, string | number>) => string;
}

const I18nContext = createContext<I18nContextType | null>(null);

const STORAGE_KEY = 'clearcore_locale';

// Helper to access nested keys like "app.title"
function getNestedValue(obj: unknown, path: string): string | undefined {
  const parts = path.split('.');
  let current: unknown = obj;
  for (const part of parts) {
    if (current && typeof current === 'object' && part in current) {
      current = (current as Record<string, unknown>)[part];
    } else {
      return undefined;
    }
  }
  return typeof current === 'string' ? current : undefined;
}

export const I18nProvider: React.FC<{ children: ReactNode }> = ({ children }) => {
  const [locale, setLocaleState] = useState<LocaleCode>(() => {
    if (typeof localStorage !== 'undefined') {
      const saved = localStorage.getItem(STORAGE_KEY);
      if (saved && availableLocales.some((l) => l.code === saved)) {
        return saved;
      }
    }
    return defaultLocaleCode;
  });

  const setLocale = (code: LocaleCode) => {
    if (availableLocales.some((l) => l.code === code)) {
      setLocaleState(code);
      if (typeof localStorage !== 'undefined') {
        localStorage.setItem(STORAGE_KEY, code);
      }
    }
  };

  const currentMeta = useMemo(() => getLocaleMeta(locale), [locale]);
  const defaultMeta = useMemo(() => getLocaleMeta(defaultLocaleCode), []);

  const t = useMemo(() => {
    return (path: string, params?: Record<string, string | number>): string => {
      // 1. Try in current locale
      let template = getNestedValue(currentMeta.translations, path);
      // 2. Fallback to default locale (pt-BR)
      if (template === undefined) {
        template = getNestedValue(defaultMeta.translations, path);
      }
      // 3. Fallback to path itself
      if (template === undefined) {
        return path;
      }

      // Replace {param} placeholders
      if (params) {
        return Object.entries(params).reduce<string>((acc, [k, v]) => {
          return acc.split(`{${k}}`).join(String(v));
        }, template);
      }

      return template;
    };
  }, [currentMeta, defaultMeta]);

  return (
    <I18nContext.Provider
      value={{
        locale,
        setLocale,
        currentMeta,
        availableLocales,
        t,
      }}
    >
      {children}
    </I18nContext.Provider>
  );
};

export const useI18n = (): I18nContextType => {
  const context = useContext(I18nContext);
  if (!context) {
    throw new Error('useI18n must be used within an I18nProvider');
  }
  return context;
};
