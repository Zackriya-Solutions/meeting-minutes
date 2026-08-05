"use client";

import type { ReactNode } from 'react';
import { useT } from '@/lib/i18n';
import type { AnalyticsNumberRow } from '@/hooks/meeting-details/useMeetingAnalyticsSections';
import { MomentLink, SectionHeading, SectionPlaceholder, StatusChip, type Tone } from './primitives';

/**
 * The «Числа встречи» tab: every figure the meeting quoted, with the moment it was said and
 * the reviewer's note on it. Same rows as the report's table; the moment jumps into the
 * transcript instead of scrolling an anchor.
 */

const CHECK_TONE: Record<string, Tone> = {
  ok: 'good',
  warn: 'warn',
};

export function MeetingNumbersPanel({
  numbers,
  onSeek,
  action,
}: {
  numbers: AnalyticsNumberRow[];
  onSeek?: (seconds: number) => void;
  /** Trailing heading slot, used for "Открыть отчёт". */
  action?: ReactNode;
}) {
  const t = useT();

  return (
    <div className="mx-auto flex max-w-[720px] flex-col gap-3 pb-10 pt-1.5">
      <SectionHeading note={t('Every figure quoted in the meeting')} action={action}>
        {t('Meeting numbers')}
      </SectionHeading>

      {numbers.length === 0 ? (
        <SectionPlaceholder>{t('No numeric claims found.')}</SectionPlaceholder>
      ) : (
        <div className="overflow-hidden rounded-[14px] border border-[var(--primary-10)]">
          <table className="w-full border-collapse text-[length:var(--ui-body-font-size)]">
            <thead>
              <tr className="bg-[var(--primary-5)] text-left text-xs text-[var(--primary-50)]">
                <th scope="col" className="px-3 py-2 font-normal">{t('Metric')}</th>
                <th scope="col" className="px-3 py-2 font-normal">{t('Figure')}</th>
                <th scope="col" className="px-3 py-2 font-normal">{t('Moment')}</th>
                <th scope="col" className="px-3 py-2 font-normal">{t('Note')}</th>
              </tr>
            </thead>
            <tbody>
              {numbers.map((row, index) => (
                <tr
                  key={`${index}-${row.metric}-${row.value}`}
                  className="border-t border-[var(--primary-10)] align-top"
                >
                  <td className="px-3 py-2.5 text-foreground">{row.metric}</td>
                  <td className="mm-numeric whitespace-nowrap px-3 py-2.5 text-foreground">
                    {row.value}
                  </td>
                  <td className="px-3 py-2.5">
                    <MomentLink
                      atSeconds={row.at_seconds}
                      onSeek={onSeek}
                      label={t('Open this moment')}
                    />
                  </td>
                  <td className="px-3 py-2.5 text-[var(--primary-60)]">
                    {row.check ? (
                      <StatusChip tone={CHECK_TONE[row.status] ?? 'neutral'}>{row.check}</StatusChip>
                    ) : null}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
