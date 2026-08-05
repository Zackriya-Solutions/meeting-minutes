"use client";

import { type ComponentProps, useEffect, useMemo, useState } from 'react';
import { Calligraph } from 'calligraph';
import { SummaryPanel } from '@/components/MeetingDetails/SummaryPanel';
import { Copy, RefreshCw } from '@/components/deslop-icons';
import { ChatMarkdown } from '@/components/chat/ChatMarkdown';
import { Button as FluidButton } from '@/components/ui/fluid-button';
import { FluidSpinner } from '@/components/ui/fluid-spinner';
import { Progress } from '@/components/ui/progress';
import MiniAppMarkdown from '@/vendor/deslop/mini-app/components/Markdown';
import {
  extractSummaryAgreementRows,
  extractSummaryParticipants,
  extractSummaryTiming,
  summaryToMarkdown,
  splitSummaryLead,
} from '@/lib/summaryToMarkdown';
import { useT } from '@/lib/i18n';
import { localizeSpeakerLabel, type SpeakerInfo } from '@/types';

import styles from './SummaryMessage.module.css';

/**
 * Summary for the meeting conversation (variant 3a): flows as content in the feed —
 * The complete overview is rendered first, followed by a bounded agreements block.
 * Picking a template regenerates both from the same persisted report.
 */

type SummaryPanelProps = ComponentProps<typeof SummaryPanel>;

interface SummaryMessageProps {
  summaryPanelProps: SummaryPanelProps;
  actualDurationSeconds?: number | null;
  speakers?: SpeakerInfo[];
}

type ActionIconProps = { size?: number; strokeWidth?: number; className?: string };
const RegenerateIcon = ({ size = 16, strokeWidth = 1.5, className }: ActionIconProps) => (
  <RefreshCw size={size} strokeWidth={strokeWidth} className={className} />
);
const CopyIcon = ({ size = 16, strokeWidth = 1.5, className }: ActionIconProps) => (
  <Copy size={size} strokeWidth={strokeWidth} className={className} />
);

function formatDuration(seconds: number): string {
  const roundedMinutes = Math.max(1, Math.round(seconds / 60));
  if (roundedMinutes < 60) return `${roundedMinutes} мин`;

  const hours = Math.floor(roundedMinutes / 60);
  const minutes = roundedMinutes % 60;
  return minutes > 0 ? `${hours} ч ${minutes} мин` : `${hours} ч`;
}

function normalizedLabel(value: string): string {
  return value.trim().toLocaleLowerCase('ru-RU').replace(/ё/g, 'е');
}

function markdownTableCell(value: string): string {
  return value.replace(/\|/g, '\\|').replace(/\r?\n/g, '<br />');
}

function engagementPercentages(speakers: SpeakerInfo[]): Map<number, number> {
  const totalDuration = speakers.reduce(
    (sum, speaker) => sum + Math.max(0, speaker.speech_duration_seconds || 0),
    0,
  );
  const totalTurns = speakers.reduce((sum, speaker) => sum + Math.max(0, speaker.segment_count), 0);
  const weighted = speakers.map((speaker) => {
    const durationShare = totalDuration > 0
      ? Math.max(0, speaker.speech_duration_seconds || 0) / totalDuration
      : 0;
    const turnShare = totalTurns > 0 ? Math.max(0, speaker.segment_count) / totalTurns : 0;
    return { id: speaker.id, score: durationShare * 0.7 + turnShare * 0.3 };
  });
  const scoreTotal = weighted.reduce((sum, item) => sum + item.score, 0);
  if (scoreTotal <= 0) return new Map();

  const apportioned = weighted.map((item, index) => {
    const exact = (item.score / scoreTotal) * 100;
    return { ...item, index, percent: Math.floor(exact), remainder: exact - Math.floor(exact) };
  });
  let unassigned = 100 - apportioned.reduce((sum, item) => sum + item.percent, 0);
  [...apportioned]
    .sort((left, right) => right.remainder - left.remainder || left.index - right.index)
    .forEach((item) => {
      if (unassigned <= 0) return;
      apportioned[item.index].percent += 1;
      unassigned -= 1;
    });

  return new Map(apportioned.map((item) => [item.id, item.percent]));
}

export function SummaryMessage({ summaryPanelProps: p, actualDurationSeconds, speakers = [] }: SummaryMessageProps) {
  const t = useT();
  const [animateEngagement, setAnimateEngagement] = useState(false);

  const isGenerating =
    p.summaryStatus === 'processing' || p.summaryStatus === 'summarizing' || p.summaryStatus === 'regenerating';
  const hasSummary = !!p.aiSummary;

  const content = useMemo(() => {
    if (!hasSummary) return { participants: '', lead: '', timing: '', agreements: [] };
    const markdown = summaryToMarkdown(p.aiSummary);
    return {
      participants: extractSummaryParticipants(
        markdown,
        (label) => localizeSpeakerLabel(label, t) ?? label,
      ),
      lead: splitSummaryLead(markdown).lead,
      timing: extractSummaryTiming(markdown),
      agreements: extractSummaryAgreementRows(markdown),
    };
  }, [hasSummary, p.aiSummary, t]);

  const participants = useMemo(
    () => content.participants
      .split('\n')
      .map((line) => line.replace(/^\s*[-+*]\s+/, '').trim())
      .filter(Boolean),
    [content.participants],
  );
  const participantAnimationKey = participants.join('\u0000');
  const agreementsTable = useMemo(() => [
    `| ${t('Who')} | ${t('What')} | ${t('When')} |`,
    '| --- | --- | --- |',
    ...content.agreements.map(({ who, what, when }) =>
      `| ${markdownTableCell(who)} | ${markdownTableCell(what)} | ${markdownTableCell(when)} |`),
  ].join('\n'), [content.agreements, t]);

  useEffect(() => {
    if (participants.length === 0) return;

    setAnimateEngagement(false);
    const frame = window.requestAnimationFrame(() => setAnimateEngagement(true));
    return () => window.cancelAnimationFrame(frame);
  }, [participantAnimationKey, participants.length]);

  const engagementByLabel = useMemo(() => {
    const percentages = engagementPercentages(speakers);
    const byLabel = new Map<string, number>();
    speakers.forEach((speaker) => {
      const label = normalizedLabel(
        localizeSpeakerLabel(speaker.is_self ? 'You' : speaker.display_name, t) ?? speaker.display_name,
      );
      const percent = percentages.get(speaker.id);
      if (percent != null) byLabel.set(label, (byLabel.get(label) ?? 0) + percent);
    });
    return byLabel;
  }, [speakers, t]);

  const participantRows = useMemo(() => participants
    .map((participant, index) => ({
      participant,
      index,
      engagement: engagementByLabel.get(normalizedLabel(participant)),
    }))
    .sort((left, right) =>
      (right.engagement ?? -1) - (left.engagement ?? -1) || left.index - right.index),
  [engagementByLabel, participants]);

  return (
    <div>
      {isGenerating ? (
        <div className="flex items-center gap-2 py-4 text-sm text-muted-foreground">
          <FluidSpinner className="h-4 w-4 text-primary" />
          {p.getSummaryStatusMessage(p.summaryStatus)}
        </div>
      ) : hasSummary ? (
        <div className="flex flex-col gap-6">
          {content.lead ? (
            <section>
              <h2 className="text-sm font-normal leading-5 text-[var(--primary-50)]">{t('About the meeting')}</h2>
              <ChatMarkdown content={content.lead} className="mm-summary-plain mt-2" />
            </section>
          ) : null}
          {participants.length > 0 ? (
            <section>
              <h2 className="text-sm font-normal leading-5 text-[var(--primary-50)]">
                {t(participants.length === 1 ? 'Participants' : 'Participant engagement')}
              </h2>
              <div className="mt-3 flex flex-col gap-3 text-[length:var(--ui-body-font-size)] leading-[21px] text-foreground">
                {participantRows.map(({ participant, engagement }) => {
                  return (
                    <div key={participant}>
                      <div className="flex min-w-0 items-center justify-between gap-2">
                        <span className="min-w-0 truncate">{participant}</span>
                        {participants.length > 1 && engagement != null && (
                          <span className="mm-numeric inline-flex min-w-[4ch] shrink-0 justify-end text-[var(--primary-40)]">
                            <Calligraph variant="number" animation="bouncy">
                              {animateEngagement ? engagement : 0}
                            </Calligraph>
                            <span>%</span>
                          </span>
                        )}
                      </div>
                      {participants.length > 1 ? (
                        <Progress
                          value={animateEngagement ? (engagement ?? 0) : 0}
                          aria-label={`${participant}: ${engagement ?? 0}%`}
                          className="mt-1 gap-0"
                        />
                      ) : null}
                    </div>
                  );
                })}
              </div>
            </section>
          ) : null}
          <section>
            <h2 className="text-sm font-normal leading-5 text-[var(--primary-50)]">{t('Timing')}</h2>
            <ChatMarkdown
              content={[
                actualDurationSeconds && actualDurationSeconds > 0
                  ? `- ${t('Actual duration')}: ${formatDuration(actualDurationSeconds)}`
                  : '',
                content.timing || `- ${t('Planned timing was not specified.')}`,
              ].filter(Boolean).join('\n')}
              className="mm-summary-list mt-2"
            />
          </section>
          {content.agreements.length > 0 ? (
            <section>
              <h2 className="text-sm font-normal leading-5 text-[var(--primary-50)]">{t('Agreements')}</h2>
              <div
                className={`mt-2 ${styles.agreementsTable}`}
              >
                <MiniAppMarkdown>{agreementsTable}</MiniAppMarkdown>
              </div>
            </section>
          ) : null}

          <div className="flex gap-1">
            <FluidButton
              type="button"
              variant="secondary"
              size="sm"
              leadingIcon={RegenerateIcon}
              className="text-[var(--primary-60)]"
              onClick={() => void p.onRegenerateSummary()}
            >
              {t('Regenerate')}
            </FluidButton>
            <FluidButton
              type="button"
              variant="secondary"
              size="sm"
              leadingIcon={CopyIcon}
              className="text-[var(--primary-60)]"
              onClick={() => void p.onCopySummary()}
            >
              {t('Copy')}
            </FluidButton>
          </div>
        </div>
      ) : p.summaryLoadStatus === 'loading' ? (
        <div className="flex items-center gap-2 py-4 text-sm text-muted-foreground">
          <FluidSpinner className="h-4 w-4 text-primary" />
          {t('Loading saved summary...')}
        </div>
      ) : null}

      {p.summaryError && !isGenerating && hasSummary && (
        <p className="mt-2 text-xs text-destructive">{p.summaryError}</p>
      )}
    </div>
  );
}
