"use client";

import {
  type ComponentProps,
  type ReactNode,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from 'react';
import { SummaryPanel } from '@/components/MeetingDetails/SummaryPanel';
import { RefreshCw } from '@/components/deslop-icons';
import { ChatMarkdown } from '@/components/chat/ChatMarkdown';
import { Button as FluidButton } from '@/components/ui/fluid-button';
import { FluidSpinner } from '@/components/ui/fluid-spinner';
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip';
import {
  extractSummaryAgreements,
  summaryToMarkdown,
  splitSummaryLead,
} from '@/lib/summaryToMarkdown';
import { useT } from '@/lib/i18n';
import { isUnresolvedSpeakerLabel, type SpeakerInfo } from '@/types';
import type { AnalyticsRoleRow } from '@/hooks/meeting-details/useMeetingAnalyticsSections';
import Cell, { CellText } from '@/vendor/deslop/mini-app/Cell';
import CellStack from '@/vendor/deslop/mini-app/components/CellStack';
import InitialsAvatar from '@/vendor/deslop/mini-app/components/InitialsAvatar';
import MotionProvider from '@/vendor/deslop/mini-app/components/MotionProvider';

/**
 * Summary for the meeting conversation (variant 3a): flows as content in the feed —
 * The complete overview is rendered first, followed by a bounded agreements block.
 * Picking a template regenerates both from the same persisted report.
 */

type SummaryPanelProps = ComponentProps<typeof SummaryPanel>;

interface SummaryMessageProps {
  summaryPanelProps: SummaryPanelProps;
  speakers: SpeakerInfo[];
  roles?: AnalyticsRoleRow[];
}

type ActionIconProps = { size?: number; strokeWidth?: number; className?: string };
const RegenerateIcon = ({ size = 16, strokeWidth = 1.5, className }: ActionIconProps) => (
  <RefreshCw size={size} strokeWidth={strokeWidth} className={className} />
);

function ParticipantAvatar({ userId, name }: { userId: number; name: string }) {
  const firstWord = name.trim().split(/\s+/, 1)[0] ?? name;
  return <InitialsAvatar size={32} userId={userId} name={firstWord} />;
}

function RenameHint({ children }: { children: ReactNode }) {
  const t = useT();
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <span className="border-b border-dashed border-[var(--primary-30)]">
          {children}
        </span>
      </TooltipTrigger>
      <TooltipContent>{t('Rename')}</TooltipContent>
    </Tooltip>
  );
}

function compactParticipantPreview(participants: readonly string[]): string {
  const visible = participants.slice(0, 2).join(', ');
  const remaining = participants.length - 2;
  return remaining > 0
    ? `${visible}\nи ещё ${remaining}`
    : visible;
}

function ParticipantPreview({ participants }: { participants: readonly string[] }) {
  const containerRef = useRef<HTMLSpanElement>(null);
  const measureRef = useRef<HTMLSpanElement>(null);
  const [allFit, setAllFit] = useState(false);
  const fullText = participants.join(', ');

  const measure = useCallback(() => {
    const container = containerRef.current;
    const fullLine = measureRef.current;
    if (!container || !fullLine) return;
    setAllFit(fullLine.scrollWidth <= container.clientWidth + 1);
  }, [fullText]);

  useEffect(() => {
    measure();
    const container = containerRef.current;
    if (!container || typeof ResizeObserver === 'undefined') return;
    const observer = new ResizeObserver(measure);
    observer.observe(container);
    return () => observer.disconnect();
  }, [measure]);

  return (
    <span ref={containerRef} className="relative block min-w-0 w-full whitespace-pre-line">
      <span
        ref={measureRef}
        aria-hidden="true"
        className="pointer-events-none invisible absolute left-0 top-0 whitespace-nowrap"
      >
        {fullText}
      </span>
      <RenameHint>{allFit ? fullText : compactParticipantPreview(participants)}</RenameHint>
    </span>
  );
}

const NON_NAMES = new Set(['ну', 'вот', 'сказали']);

function isResolvedParticipantName(name: string): boolean {
  const normalized = name.trim().toLocaleLowerCase('ru-RU');
  return !isUnresolvedSpeakerLabel(name) && !NON_NAMES.has(normalized);
}

export function SummaryMessage({ summaryPanelProps: p, speakers, roles = [] }: SummaryMessageProps) {
  const t = useT();

  const isGenerating =
    p.summaryStatus === 'processing' || p.summaryStatus === 'summarizing' || p.summaryStatus === 'regenerating';
  const hasSummary = !!p.aiSummary;
  const failureMessage = isGenerating ? null : (p.summaryError || p.summaryLoadError);

  const content = useMemo(() => {
    if (!hasSummary) return { lead: '', agreements: '' };
    const markdown = summaryToMarkdown(p.aiSummary);
    return {
      lead: splitSummaryLead(markdown).lead,
      agreements: extractSummaryAgreements(markdown),
    };
  }, [hasSummary, p.aiSummary]);

  const participants = useMemo(
    () => speakers
      .map((speaker) => speaker.display_name.trim())
      .filter(isResolvedParticipantName)
      .filter((name, index, names) => names.indexOf(name) === index),
    [speakers],
  );
  const participantRoles = useMemo(() => new Map(
    roles.map((role) => [role.speaker.trim().toLocaleLowerCase('ru-RU'), role.role]),
  ), [roles]);
  const roleFor = (participant: string) => (
    participantRoles.get(participant.trim().toLocaleLowerCase('ru-RU'))
  );
  return (
    <div>
      {isGenerating ? (
        <div className="flex items-center gap-2 py-4 text-sm text-muted-foreground">
          <FluidSpinner className="h-4 w-4 text-primary" />
          {p.getSummaryStatusMessage(p.summaryStatus)}
        </div>
      ) : hasSummary ? (
        <div className="flex flex-col gap-6">
          {participants.length > 1 ? (
            <section className="apple mb-5" data-mini-app>
              <h2 className="text-sm font-normal leading-5 text-[var(--primary-50)]">{t('Participants')}</h2>
              <div className="mt-2">
                <MotionProvider strict={false}>
                  <CellStack>
                    <CellStack.Morph>
                      <Cell
                        type="button"
                        start={<ParticipantAvatar userId={1} name={participants[1]} />}
                      >
                        <CellText
                          title={(
                            <ParticipantPreview participants={participants} />
                          )}
                        />
                      </Cell>
                      <Cell
                        type="button"
                        start={<ParticipantAvatar userId={0} name={participants[0]} />}
                      >
                        <CellText
                          title={<RenameHint>{participants[0]}</RenameHint>}
                          description={roleFor(participants[0])}
                        />
                      </Cell>
                    </CellStack.Morph>
                    {participants.slice(1).map((participant, index) => (
                      <Cell
                        key={participant}
                        type="button"
                        start={<ParticipantAvatar userId={index + 1} name={participant} />}
                      >
                        <CellText
                          title={<RenameHint>{participant}</RenameHint>}
                          description={roleFor(participant)}
                        />
                      </Cell>
                    ))}
                  </CellStack>
                </MotionProvider>
              </div>
            </section>
          ) : null}
          {content.lead ? (
            <section>
              <h2 className="text-sm font-normal leading-5 text-[var(--primary-50)]">{t('About the meeting')}</h2>
              <ChatMarkdown content={content.lead} className="mm-summary-plain mt-2" />
            </section>
          ) : null}
          {content.agreements ? (
            <section>
              <h2 className="text-sm font-normal leading-5 text-[var(--primary-50)]">{t('Agreements')}</h2>
              <ChatMarkdown
                content={content.agreements}
                className="mm-summary-list mm-summary-plain mt-2"
              />
            </section>
          ) : null}

        </div>
      ) : p.summaryLoadStatus === 'loading' ? (
        <div className="flex items-center gap-2 py-4 text-sm text-muted-foreground">
          <FluidSpinner className="h-4 w-4 text-primary" />
          {t('Loading saved summary...')}
        </div>
      ) : failureMessage ? (
        // A run that ended without a summary must say so and offer a way forward: silently
        // replacing the spinner with an empty panel reads as "this meeting has nothing".
        <div className="flex flex-col items-start gap-3 py-4">
          <p className="text-sm text-muted-foreground">{failureMessage}</p>
          <FluidButton
            type="button"
            variant="secondary"
            size="sm"
            leadingIcon={RegenerateIcon}
            className="text-[var(--primary-60)]"
            onClick={() => void p.onGenerateSummary('')}
          >
            {t('Generate Summary')}
          </FluidButton>
        </div>
      ) : null}

      {p.summaryError && !isGenerating && hasSummary && (
        <p className="mt-2 text-xs text-destructive">{p.summaryError}</p>
      )}
    </div>
  );
}
