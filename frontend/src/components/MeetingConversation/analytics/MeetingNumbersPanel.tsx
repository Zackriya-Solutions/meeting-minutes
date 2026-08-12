"use client";

import type { ComponentType, ReactNode } from 'react';
import { useT } from '@/lib/i18n';
import type { AnalyticsNumberRow } from '@/hooks/meeting-details/useMeetingAnalyticsSections';
import Table from '@/vendor/deslop/mini-app/components/Table';
import { MomentLink, SectionHeading, SectionPlaceholder } from './primitives';

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

export function MeetingNumbersPanel({
  numbers,
  onSeek,
}: {
  numbers: AnalyticsNumberRow[];
  onSeek?: (seconds: number) => void;
}) {
  const t = useT();

  return (
    <div className="mx-auto flex max-w-[720px] flex-col gap-3 pb-10 pt-1.5">
      <SectionHeading>
        {t('Meeting numbers')}
      </SectionHeading>

      {numbers.length === 0 ? (
        <SectionPlaceholder>{t('No numeric claims found.')}</SectionPlaceholder>
      ) : (
        <div className="apple" data-mini-app>
          <DeslopTable
            head={[t('Metric'), t('Figure'), t('Moment')]}
            rows={numbers.map((row) => [
              row.metric,
              <span key="value" className="mm-numeric">{row.value}</span>,
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
      )}
    </div>
  );
}
