'use client';

import { useState } from 'react';
import { Globe, Pin } from '@/components/memento/LucideCompat';
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
    <div className="bg-[var(--bg-canvas)] rounded-lg border border-[var(--border-subtle)] p-6 shadow-none relative">
      <div className="flex items-center gap-2 mb-2">
        <Globe size={18} className="text-[var(--fg2)]" />
        <h3 className="text-lg font-semibold text-[var(--fg1)]">{t('Summary Language')}</h3>
      </div>
      <p className="text-sm text-[var(--fg2)] mb-4">
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
                  ? 'bg-[var(--gold-soft)] border-[var(--gold-border)] text-[var(--gold)]'
                  : 'bg-[var(--bg-elevated)] border-[var(--border-subtle)] text-[var(--fg1)]'
              }`}
            >
              <button
                type="button"
                aria-label={isPinned ? `${t('Unpin')} ${labelForCode(code)} ${t('as default')}` : `${t('Pin')} ${labelForCode(code)} ${t('as default')}`}
                aria-pressed={isPinned}
                title={isPinned ? t('Click to unset as default') : t('Click to set as default')}
                onClick={() => togglePin(code)}
                className={`flex items-center gap-1.5 pl-3 pr-2 py-1 hover:brightness-95 active:brightness-90 ${
                  isPinned ? 'text-[var(--gold)]' : 'text-[var(--fg1)]'
                }`}
              >
                <Pin
                  size={14}
                  className={isPinned ? 'text-[var(--gold)]' : 'text-[var(--fg3)]'}
                  fill={isPinned ? 'currentColor' : 'none'}
                />
                {labelForCode(code)}
              </button>
              <button
                type="button"
                aria-label={`${t('Remove')} ${labelForCode(code)}`}
                onClick={() => removeRecent(code)}
                className={`pr-2.5 pl-0.5 py-1 leading-none ${isPinned ? 'text-[var(--gold)] hover:text-[var(--gold-active)]' : 'text-[var(--fg3)] hover:text-[var(--fg2)]'}`}
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
              className="inline-flex items-center gap-1 rounded-full border border-dashed border-[var(--border-strong)] px-3 py-1 text-sm text-[var(--fg2)] hover:border-[var(--gold-border)] hover:text-[var(--fg1)] disabled:cursor-not-allowed disabled:opacity-50"
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

      <p className="text-xs text-[var(--fg3)] mt-3">
        {pinned
          ? `${t('Default:')} ${labelForCode(pinned)} - ${t('click it again to unset. Max 5 quick-switch options.')}`
          : t('Click any language to set it as your default. Max 5 quick-switch options.')}
      </p>
    </div>
  );
}
