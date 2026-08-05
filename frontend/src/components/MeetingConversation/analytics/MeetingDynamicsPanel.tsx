"use client";

import type { ReactNode } from 'react';
import { useT } from '@/lib/i18n';
import { Progress } from '@/components/ui/progress';
import type {
  AnalyticsDynamicsSection,
  AnalyticsRoleRow,
} from '@/hooks/meeting-details/useMeetingAnalyticsSections';
import {
  MomentLink,
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
  roles,
  onSeek,
  action,
}: {
  /** null only if the (local) dynamics stage is missing from an old snapshot. */
  dynamics: AnalyticsDynamicsSection | null;
  roles: AnalyticsRoleRow[];
  onSeek?: (seconds: number) => void;
  /** Trailing heading slot, used for "Открыть отчёт". */
  action?: ReactNode;
}) {
  const t = useT();
  const density = dynamics
    ? Math.round(Math.max(0, Math.min(1, dynamics.speech_density)) * 100)
    : 0;
  const speakers = dynamics?.speakers ?? [];
  const questionsBySpeaker = speakers
    .filter((speaker) => speaker.questions > 0)
    .map((speaker) => `${speaker.label} — ${speaker.questions}`)
    .join(', ');

  return (
    <div className="mx-auto flex max-w-[720px] flex-col gap-7 pb-10 pt-1.5">
      <section>
        <SectionHeading note={t('Deterministic analytics, no LLM')} action={action}>
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

      <section>
        <SectionHeading>{t('Who talked — share of time')}</SectionHeading>
        {speakers.length === 0 ? (
          <div className="mt-2">
            <SectionPlaceholder>{t('No speech data.')}</SectionPlaceholder>
          </div>
        ) : (
          <>
            <div className="mt-3 flex flex-col gap-3 text-[length:var(--ui-body-font-size)] leading-[21px] text-foreground">
              {speakers.map((speaker) => {
                const share = Math.round(Math.max(0, Math.min(1, speaker.talk_share)) * 100);
                return (
                  <div key={`${speaker.label}-${speaker.talk_secs}`}>
                    <div className="flex min-w-0 items-center justify-between gap-2">
                      <span className="min-w-0 truncate">{speaker.label}</span>
                      <span className="mm-numeric shrink-0 text-[var(--primary-40)]">
                        {formatClock(speaker.talk_secs)}&nbsp;· {share}%
                      </span>
                    </div>
                    <Progress
                      value={share}
                      aria-label={`${speaker.label}: ${share}%`}
                      className="mt-1 gap-0"
                    />
                  </div>
                );
              })}
            </div>
            {questionsBySpeaker && (
              <p className="mt-3 text-xs text-[var(--primary-40)]">
                {t('Questions by speaker')}: {questionsBySpeaker}.
              </p>
            )}
          </>
        )}
      </section>

      <section>
        <SectionHeading note={t('Behaviour in this meeting, not a trait')}>
          {t('Roles in the meeting')}
        </SectionHeading>
        {roles.length === 0 ? (
          <div className="mt-2">
            <SectionPlaceholder>{t('Roles could not be determined.')}</SectionPlaceholder>
          </div>
        ) : (
          <div className="mt-3 flex flex-col gap-2">
            {roles.map((role, index) => (
              <RoleCard key={`${index}-${role.speaker}`} role={role} onSeek={onSeek} />
            ))}
          </div>
        )}
      </section>
    </div>
  );
}

function RoleCard({
  role,
  onSeek,
}: {
  role: AnalyticsRoleRow;
  onSeek?: (seconds: number) => void;
}) {
  const t = useT();
  return (
    <div className="rounded-[14px] bg-[var(--primary-5)] px-3.5 py-3">
      <div className="flex flex-wrap items-baseline gap-x-2 gap-y-1">
        <span className="text-[length:var(--ui-body-font-size)] font-medium leading-[21px] text-foreground">
          {role.speaker}
        </span>
        <span className="text-xs text-[var(--primary-50)]">{role.role}</span>
        <span className="ml-auto">
          <MomentLink atSeconds={role.at_seconds} onSeek={onSeek} label={t('Open this moment')} />
        </span>
      </div>
      {role.evidence && (
        <p className="mt-1 text-[length:var(--ui-body-font-size)] leading-[21px] text-[var(--primary-60)]">
          {role.evidence}
        </p>
      )}
    </div>
  );
}

/** A failed stage shows "—" rather than a zero it cannot vouch for. */
function countLabel(value: number | null): string {
  return value == null ? '—' : String(value);
}
