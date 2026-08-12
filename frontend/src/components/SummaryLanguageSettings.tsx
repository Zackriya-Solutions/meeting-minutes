'use client';

import { Globe } from '@/components/deslop-icons';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
} from '@/components/ui/fluid-select';
import { useRecentLanguages } from '@/hooks/useRecentLanguages';
import { AUTO_VALUE, LANGUAGE_OPTIONS } from '@/lib/summary-languages';
import { useT } from '@/lib/i18n';

export function SummaryLanguageSettings() {
  const t = useT();
  const { pinned, addRecent, setPinned } = useRecentLanguages();

  const handleLanguageChange = (value: string) => {
    if (value === AUTO_VALUE) {
      setPinned(null);
      return;
    }

    setPinned(value);
    addRecent(value);
  };

  return (
    <section className="settings-section settings-cell">
      <div className="settings-cell__row">
        <span className="settings-cell__avatar" aria-hidden="true">
          <Globe size={20} />
        </span>
        <div className="settings-cell__text">
          <h3 className="settings-cell__label">{t('Summary Language')}</h3>
          <p className="settings-cell__caption">{t('Language for new meeting summaries')}</p>
        </div>
        <div className="settings-cell__control">
          <Select shape="rounded" value={pinned ?? AUTO_VALUE} onValueChange={handleLanguageChange}>
            <SelectTrigger
              className="settings-cell__select settings-cell__device-select"
              placeholder={t('Auto')}
            />
            <SelectContent>
              <SelectItem index={0} value={AUTO_VALUE}>{t('Auto')}</SelectItem>
              {LANGUAGE_OPTIONS.map(({ code, label }, index) => (
                <SelectItem key={code} index={index + 1} value={code}>
                  {label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
      </div>
    </section>
  );
}
