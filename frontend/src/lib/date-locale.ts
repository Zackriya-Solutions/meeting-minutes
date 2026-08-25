import { enUS, zhCN } from 'date-fns/locale';
import type { Locale } from 'date-fns';
import { useLocale } from 'next-intl';

const dateFnsLocales: Record<string, Locale> = {
  'en-US': enUS,
  'zh-CN': zhCN,
};

export const getDateFnsLocale = (intlLocale: string): Locale =>
  dateFnsLocales[intlLocale] ?? enUS;

export const useDateFnsLocale = (): Locale => getDateFnsLocale(useLocale());
