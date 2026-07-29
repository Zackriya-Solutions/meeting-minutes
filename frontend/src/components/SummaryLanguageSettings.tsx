'use client';

import { useState } from 'react';
import { Globe, Pin } from '@/components/deslop-icons';
import { Popover, PopoverTrigger, PopoverContent } from '@/components/ui/popover';
import { LanguagePickerPopover } from '@/components/LanguagePickerPopover';
import { useRecentLanguages } from '@/hooks/useRecentLanguages';
import { labelForCode } from '@/lib/summary-languages';
import { useT } from '@/lib/i18n';

export function SummaryLanguageSettings() {
  const t = useT();
  const { recents, pinned, addRecent, removeRecent, setPinned } = useRecentLanguages();
  const [pickerOpen, setPickerOpen] = useState(false);

  const togglePin = (code: string) => {
    setPinned(pinned === code ? null : code);
  };

  return (
    <div className="bg-background rounded-lg border border-border p-6 shadow-none relative">
      <div className="flex items-center gap-2 mb-2">
        <Globe size={18} className="text-muted-foreground" />
        <h3 className="text-lg font-semibold text-foreground">{t('Summary Language')}</h3>
      </div>
      <p className="text-sm text-muted-foreground mb-4">
        {t('Pin one language as the default for new meetings. Unpinned languages remain as quick-switch options in the summary generator. Auto uses the dominant transcript language.')}
      </p>

      <div className="flex flex-wrap items-center gap-2">
        {recents.map((code) => {
          const isPinned = pinned === code;
          return (
            <span
              key={code}
              className={`inline-flex items-center rounded-full border text-sm overflow-hidden ${
                isPinned
                  ? 'bg-primary/10 border-primary/40 text-primary'
                  : 'bg-muted border-border text-foreground'
              }`}
            >
              <button
                type="button"
                aria-label={isPinned ? `${t('Unpin')} ${labelForCode(code)} ${t('as default')}` : `${t('Pin')} ${labelForCode(code)} ${t('as default')}`}
                aria-pressed={isPinned}
                title={isPinned ? t('Click to unset as default') : t('Click to set as default')}
                onClick={() => togglePin(code)}
                className={`flex items-center gap-1.5 pl-3 pr-2 py-1 hover:brightness-95 active:brightness-90 ${
                  isPinned ? 'text-primary' : 'text-foreground'
                }`}
              >
                <Pin
                  size={14}
                  className={isPinned ? 'text-primary' : 'text-muted-foreground'}
                  fill={isPinned ? 'currentColor' : 'none'}
                />
                {labelForCode(code)}
              </button>
              <button
                type="button"
                aria-label={`${t('Remove')} ${labelForCode(code)}`}
                onClick={() => removeRecent(code)}
                className={`pr-2.5 pl-0.5 py-1 leading-none ${isPinned ? 'text-primary hover:text-primary/90' : 'text-muted-foreground hover:text-muted-foreground'}`}
              >
                ×
              </button>
            </span>
          );
        })}

        <Popover open={pickerOpen} onOpenChange={setPickerOpen}>
          <PopoverTrigger asChild>
            <button
              type="button"
              disabled={recents.length >= 5}
              className="inline-flex items-center gap-1 rounded-full border border-dashed border-border px-3 py-1 text-sm text-muted-foreground hover:border-primary/40 hover:text-foreground disabled:cursor-not-allowed disabled:opacity-50"
            >
              ＋ {t('Add language')}
            </button>
          </PopoverTrigger>
          <PopoverContent align="start" className="w-auto p-0 border-0 shadow-none bg-transparent">
            <LanguagePickerPopover
              mode="settings"
              value={null}
              onChange={(code) => {
                if (code) addRecent(code);
                setPickerOpen(false);
              }}
              onClose={() => setPickerOpen(false)}
            />
          </PopoverContent>
        </Popover>
      </div>

      <p className="text-xs text-muted-foreground mt-3">
        {pinned
          ? `${t('Default:')} ${labelForCode(pinned)} - ${t('click it again to unset. Max 5 quick-switch options.')}`
          : t('Click any language to set it as your default. Max 5 quick-switch options.')}
      </p>
    </div>
  );
}
