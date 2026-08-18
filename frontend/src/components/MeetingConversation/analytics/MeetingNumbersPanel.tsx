"use client";

import type { ComponentType, ReactNode } from 'react';
import { useT } from '@/lib/i18n';
import type { AnalyticsNumberRow } from '@/hooks/meeting-details/useMeetingAnalyticsSections';
import Table from '@/vendor/deslop/mini-app/components/Table';
import { MomentLink, SectionHeading } from './primitives';

/**
 * Every figure the meeting quoted, with the moment it was said. The moment jumps into the
 * transcript instead of scrolling an anchor.
 */

const DeslopTable = Table as unknown as ComponentType<{
  head?: ReactNode[];
  rows: ReactNode[][];
  align?: Array<'left' | 'center' | 'right'>;
  className?: string;
}>;

function capitalizeMetric(value: string): string {
  const trimmed = value.trim();
  if (!trimmed) return value;
  const characters = Array.from(trimmed);
  return `${characters[0].toLocaleUpperCase('ru-RU')}${characters.slice(1).join('')}`;
}

export function MeetingNumbersPanel({
  numbers,
  onSeek,
}: {
  numbers: AnalyticsNumberRow[];
  onSeek?: (seconds: number) => void;
}) {
  const t = useT();

  // Do not leave an empty heading or placeholder in the feed while the report
  // has no numeric claims yet (for example, when generation failed or is still
  // being populated). The section becomes visible as soon as rows arrive.
  if (numbers.length === 0) return null;

  return (
    <div className="mx-auto flex max-w-[720px] flex-col gap-3 pb-10 pt-1.5">
      <SectionHeading>
        {t('Meeting numbers')}
      </SectionHeading>

      <div className="apple" data-mini-app>
        <DeslopTable
          className="meeting-numbers-table"
          head={[t('Metric'), t('Figure'), t('Moment')]}
          rows={numbers.map((row) => [
            capitalizeMetric(row.metric),
            <span key="value">{row.value}</span>,
            <MomentLink
              key="moment"
              atSeconds={row.at_seconds}
              onSeek={onSeek}
              label={t('Open this moment')}
            />,
          ])}
          align={['left', 'left', 'left']}
        />
      </div>
    </div>
  );
}
