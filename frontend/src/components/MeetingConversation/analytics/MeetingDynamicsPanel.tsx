"use client";

import { useT } from '@/lib/i18n';
import type { AnalyticsDynamicsSection } from '@/hooks/meeting-details/useMeetingAnalyticsSections';
import {
  SectionHeading,
  SectionPlaceholder,
  StatTile,
  formatClock,
} from './primitives';

/**
 * The «Динамика встречи» tab: the deterministic half of the report — counted tiles, who
 * talked for how long, and the behavioural roles the meeting fell into. No LLM is involved
 * in the tiles or the shares; only the roles come from a model.
 *
 * The report's timeline canvas (topic bands + event markers) is deliberately not ported —
 * the transcript tab already gives that navigation, and the moment links here reach it.
 */

export function MeetingDynamicsPanel({
  dynamics,
}: {
  /** null only if the (local) dynamics stage is missing from an old snapshot. */
  dynamics: AnalyticsDynamicsSection | null;
}) {
  const t = useT();
  const density = dynamics
    ? Math.round(Math.max(0, Math.min(1, dynamics.speech_density)) * 100)
    : 0;

  return (
    <div className="mx-auto flex max-w-[720px] flex-col gap-7 pb-10 pt-1.5">
      <section>
        <SectionHeading>
          {t('Meeting dynamics')}
        </SectionHeading>
        {dynamics ? (
          <div className="mt-3 grid grid-cols-2 gap-2 sm:grid-cols-3">
            <StatTile
              label={t('Duration')}
              value={formatClock(dynamics.duration_secs)}
              sub={`${t('speech density')} ${density}%`}
            />
            <StatTile label={t('Decisions made')} value={countLabel(dynamics.decisions_count)} />
            <StatTile
              label={t('Commitments made')}
              value={countLabel(dynamics.commitments_count)}
            />
            <StatTile label={t('Questions asked')} value={String(dynamics.total_questions)} />
            <StatTile label={t('Speaking turns')} value={String(dynamics.turn_count)} />
            <StatTile
              label={t('Pauses over 10s')}
              value={String(dynamics.pauses_over_10s)}
              sub={`${t('over 3s')}: ${dynamics.pauses_over_3s}`}
            />
          </div>
        ) : (
          <div className="mt-2">
            <SectionPlaceholder>{t('No speech data.')}</SectionPlaceholder>
          </div>
        )}
      </section>

    </div>
  );
}

/** A failed stage shows "—" rather than a zero it cannot vouch for. */
function countLabel(value: number | null): string {
  return value == null ? '—' : String(value);
}
