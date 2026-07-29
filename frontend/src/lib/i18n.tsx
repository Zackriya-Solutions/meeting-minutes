'use client';

import React, { createContext, useCallback, useContext, useMemo } from 'react';
import { RU } from './translations';

export type Lang = 'ru';

interface LanguageContextValue {
  lang: Lang;
  /**
   * Translate a stable source key to Russian. The English-looking strings are
   * internal identifiers, not a user-selectable locale.
   */
  t: (en: string) => string;
}

const LanguageContext = createContext<LanguageContextValue | null>(null);

export function LanguageProvider({ children }: { children: React.ReactNode }) {
  const t = useCallback((key: string) => RU[key] ?? key, []);
  const value = useMemo<LanguageContextValue>(() => ({ lang: 'ru', t }), [t]);

  return <LanguageContext.Provider value={value}>{children}</LanguageContext.Provider>;
}

export function useLanguage(): LanguageContextValue {
  const ctx = useContext(LanguageContext);
  if (!ctx) {
    // Safe fallback if used outside the provider: Russian, no toggle.
    return { lang: 'ru', t: (key: string) => RU[key] ?? key };
  }
  return ctx;
}

/** Convenience hook when only the translate function is needed. */
export function useT(): (en: string) => string {
  return useLanguage().t;
}

/**
 * Non-hook translation for module scope where React hooks are unavailable.
 */
export function translate(key: string): string {
  return RU[key] ?? key;
}
