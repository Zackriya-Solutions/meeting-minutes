import { useCallback } from 'react';
import { useConfig } from '@/contexts/ConfigContext';
import { AppLanguage, translate } from '@/lib/app-i18n';

export function useI18n() {
  const { appLanguage, setAppLanguage } = useConfig();

  const t = useCallback(
    (key: string, params?: Record<string, string | number>) => {
      return translate(appLanguage as AppLanguage, key, params);
    },
    [appLanguage]
  );

  return {
    appLanguage: appLanguage as AppLanguage,
    setAppLanguage,
    t,
  };
}
