'use client';

import React, { createContext, useCallback, useContext, useEffect, useMemo, useState } from 'react';
import { RU } from './translations';

export type Lang = 'ru' | 'en';

const STORAGE_KEY = 'memento:language';

interface LanguageContextValue {
  lang: Lang;
  setLang: (lang: Lang) => void;
  /**
   * Translate an English source string (the key) to the active language.
   * English is the canonical key (from the upstream `main` build); Russian
   * values live in `translations.ts`. Unknown keys fall back to the key itself,
   * so a missing Russian entry renders the English text rather than blank.
   */
  t: (en: string) => string;
}

const LanguageContext = createContext<LanguageContextValue | null>(null);

function readInitialLang(): Lang {
  if (typeof window === 'undefined') return 'ru';
  const stored = window.localStorage.getItem(STORAGE_KEY);
  return stored === 'en' || stored === 'ru' ? stored : 'ru';
}

export function LanguageProvider({ children }: { children: React.ReactNode }) {
  const [lang, setLangState] = useState<Lang>('ru');

  // Read persisted preference on mount (client only) to avoid hydration drift.
  useEffect(() => {
    setLangState(readInitialLang());
  }, []);

  const setLang = useCallback((next: Lang) => {
    setLangState(next);
    try {
      window.localStorage.setItem(STORAGE_KEY, next);
    } catch {
      /* ignore persistence errors */
    }
  }, []);

  const t = useCallback(
    (en: string) => (lang === 'en' ? en : RU[en] ?? en),
    [lang],
  );

  const value = useMemo<LanguageContextValue>(() => ({ lang, setLang, t }), [lang, setLang, t]);

  return <LanguageContext.Provider value={value}>{children}</LanguageContext.Provider>;
}

export function useLanguage(): LanguageContextValue {
  const ctx = useContext(LanguageContext);
  if (!ctx) {
    // Safe fallback if used outside the provider: Russian, no toggle.
    return { lang: 'ru', setLang: () => {}, t: (en: string) => RU[en] ?? en };
  }
  return ctx;
}

/** Convenience hook when only the translate function is needed. */
export function useT(): (en: string) => string {
  return useLanguage().t;
}

/**
 * Non-hook translation for MODULE SCOPE (toast helpers, services) where the
 * React context/hook isn't available. Reads the persisted language directly, so
 * it reflects the current toggle at call time. Inside React components use
 * `useT()` instead so they re-render when the language changes.
 */
export function translate(en: string): string {
  let lang: Lang = 'ru';
  if (typeof window !== 'undefined') {
    const stored = window.localStorage.getItem(STORAGE_KEY);
    if (stored === 'en' || stored === 'ru') lang = stored;
  }
  return lang === 'en' ? en : RU[en] ?? en;
}
