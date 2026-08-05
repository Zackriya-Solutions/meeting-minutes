"use client";

import type { ReactNode } from 'react';
import { useT } from '@/lib/i18n';
import type {
  AnalyticsAgendaRow,
  MeetingAnalyticsSections,
} from '@/hooks/meeting-details/useMeetingAnalyticsSections';
import {
  Meter,
  MomentLink,
  SectionHeading,
  SectionPlaceholder,
  StatusChip,
  type Tone,
} from './primitives';

/**
 * The three report sections that read as part of the summary: the meeting score with its
 * verdict, «Что мешало», and «Покрытие повестки». Rendered under the summary message in the
 * conversation feed, in the same section language as the rest of it.
 *
 * The score judges the meeting, not the people — the kicker says so, and the meters name
 * only artefacts (owners, deadlines, definition of done, closed questions).
 */

const AGENDA_TONE: Record<string, Tone> = {
  covered: 'good',
  partial: 'warn',
  missed: 'bad',
};

const AGENDA_LABEL: Record<string, string> = {
  covered: 'Agenda item covered',
  partial: 'Agenda item partial',
  missed: 'Agenda item missed',
};

export function MeetingScoreSections({
  sections,
  onSeek,
  action,
}: {
  sections: MeetingAnalyticsSections;
  onSeek?: (seconds: number) => void;
  /** Trailing slot on the first heading, used for "Открыть отчёт". */
  action?: ReactNode;
}) {
  const t = useT();
  const { score, what_hindered: whatHindered, agenda } = sections;

  // Nothing to show when every source stage of these three sections failed.
  if (!score && whatHindered.length === 0 && agenda.length === 0) return null;

  return (
    <div className="flex flex-col gap-6">
      {score && (
        <section>
          <SectionHeading note={t('The meeting is scored, not the people')} action={action}>
            {t('Meeting score')}
          </SectionHeading>
          <div className="mt-3 grid gap-5 sm:grid-cols-[auto_1fr] sm:items-center sm:gap-8">
            <div className="flex items-end gap-1.5">
              <span className="text-[48px] font-semibold leading-none text-foreground">
                {score.total}
              </span>
              <span className="pb-1.5 text-sm text-[var(--primary-40)]">{t('out of 100')}</span>
            </div>
            <div className="flex flex-col gap-2.5">
              <Meter label={t('Score: agenda coverage')} value={score.coverage_pct} />
              <Meter label={t('Score: tasks with an owner')} value={score.owners_pct} />
              <Meter label={t('Score: tasks with a deadline')} value={score.deadline_pct} />
              <Meter label={t('Score: definition of done')} value={score.dod_pct} />
              <Meter label={t('Score: closed questions')} value={score.qa_pct} />
            </div>
          </div>
          {score.verdict && (
            <p className="mt-4 text-[length:var(--ui-body-font-size)] leading-[21px] text-foreground">
              {score.verdict}
            </p>
          )}
        </section>
      )}

      <section>
        <SectionHeading>{t('What hindered')}</SectionHeading>
        {whatHindered.length > 0 ? (
          <ul className="mt-2 flex flex-col gap-1.5">
            {whatHindered.map((item, index) => (
              <li
                key={`${index}-${item}`}
                className="flex gap-2 text-[length:var(--ui-body-font-size)] leading-[21px] text-foreground"
              >
                <span aria-hidden="true" className="text-[var(--primary-30)]">
                  —
                </span>
                <span className="min-w-0">{item}</span>
              </li>
            ))}
          </ul>
        ) : (
          <div className="mt-2">
            <SectionPlaceholder>{t('Nothing stood out.')}</SectionPlaceholder>
          </div>
        )}
      </section>

      <section>
        <SectionHeading note={t('Agenda restored from the conversation')}>
          {t('Agenda coverage')}
        </SectionHeading>
        {agenda.length > 0 ? (
          <ul className="mt-2.5 flex flex-col gap-2">
            {agenda.map((row, index) => (
              <AgendaItemRow key={`${index}-${row.item}`} row={row} onSeek={onSeek} />
            ))}
          </ul>
        ) : (
          <div className="mt-2">
            <SectionPlaceholder>{t('The agenda could not be restored.')}</SectionPlaceholder>
          </div>
        )}
      </section>
    </div>
  );
}

function AgendaItemRow({
  row,
  onSeek,
}: {
  row: AnalyticsAgendaRow;
  onSeek?: (seconds: number) => void;
}) {
  const t = useT();
  const tone = AGENDA_TONE[row.status] ?? 'neutral';
  const statusLabel = t(AGENDA_LABEL[row.status] ?? 'Agenda item missed');

  return (
    <li className="flex items-baseline gap-2 text-[length:var(--ui-body-font-size)] leading-[21px]">
      <StatusChip tone={tone} className="w-[86px] shrink-0">
        {statusLabel}
      </StatusChip>
      <span className="min-w-0 flex-1 text-foreground">{row.item}</span>
      <MomentLink atSeconds={row.at_seconds} onSeek={onSeek} label={t('Open this moment')} />
    </li>
  );
}
