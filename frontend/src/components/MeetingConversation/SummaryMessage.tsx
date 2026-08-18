"use client";

import {
  type ComponentProps,
  type MouseEvent,
  type ReactNode,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from 'react';
import { SummaryPanel } from '@/components/MeetingDetails/SummaryPanel';
import { SpeakerRenameDialog } from '@/components/MeetingDetails/SpeakerRenameDialog';
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
import {
  linkAgreementSpeakers,
  linkSpeakerNames,
  normalizedSpeakerName,
  roleForSpeaker,
} from '@/lib/summarySpeakerLinks';
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
  onRenameSpeaker?: (speakerId: number, displayName: string) => Promise<void> | void;
}

type ActionIconProps = { size?: number; strokeWidth?: number; className?: string };
const RegenerateIcon = ({ size = 16, strokeWidth = 1.5, className }: ActionIconProps) => (
  <RefreshCw size={size} strokeWidth={strokeWidth} className={className} />
);

function ParticipantAvatar({ userId, name }: { userId: number; name: string }) {
  const firstWord = name.trim().split(/\s+/, 1)[0] ?? name;
  return <InitialsAvatar size={32} userId={userId} name={firstWord} />;
}

function RenameHint({ children, onClick }: { children: ReactNode; onClick: () => void }) {
  const t = useT();

  // The name sits inside CellStack, whose root toggles the stack on any click.
  // Renaming is the name's own action, so it must not also fold the card.
  const handleClick = (event: MouseEvent<HTMLButtonElement>) => {
    event.stopPropagation();
    onClick();
  };

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <button
          type="button"
          className="speaker-rename-underline cursor-pointer bg-transparent p-0 text-left font-inherit hover:text-[var(--deslop-primary)] focus:outline-none focus-visible:ring-2 focus-visible:ring-[var(--primary-20)]"
          onClick={handleClick}
        >
          {children}
        </button>
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
      <span>{allFit ? fullText : compactParticipantPreview(participants)}</span>
    </span>
  );
}

const NON_NAMES = new Set(['ну', 'вот', 'сказали']);

/**
 * Speech a voice must hold before it counts as somebody who was at the meeting.
 *
 * Diarization deliberately leaves stranded fragments as their own cluster rather than risk
 * folding two people into one, so a meeting of three can end up with five voices, the extras
 * holding a few seconds each. They belong in the transcript — the words were said by
 * somebody — but listing them as participants turns a passing noise into a person. Measured
 * on this archive: 20 of 191 voices hold under 15 seconds, on 17 of 68 meetings.
 */
const PARTICIPANT_MIN_SPEECH_SECONDS = 15;

function isMeetingParticipant(speaker: SpeakerInfo): boolean {
  // A voice the user named or claimed is a person by assertion, however briefly it spoke.
  if (speaker.is_confirmed || speaker.is_self) return true;
  return (speaker.speech_duration_seconds ?? 0) >= PARTICIPANT_MIN_SPEECH_SECONDS;
}

function isResolvedParticipantName(name: string): boolean {
  const normalized = name.trim().toLocaleLowerCase('ru-RU');
  return !isUnresolvedSpeakerLabel(name) && !NON_NAMES.has(normalized);
}

export function SummaryMessage({
  summaryPanelProps: p,
  speakers,
  roles = [],
  onRenameSpeaker,
}: SummaryMessageProps) {
  const t = useT();
  const [renamingSpeaker, setRenamingSpeaker] = useState<SpeakerInfo | null>(null);

  const isGenerating =
    p.summaryStatus === 'processing' || p.summaryStatus === 'summarizing' || p.summaryStatus === 'regenerating';
  const isSummaryLoading = p.summaryLoadStatus === 'loading' || isGenerating;
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
      .filter(isMeetingParticipant)
      .map((speaker) => speaker.display_name.trim())
      .filter(isResolvedParticipantName)
      .filter((name, index, names) => names.indexOf(name) === index),
    [speakers],
  );
  const participantRoles = useMemo(() => new Map(
    roles.map((role) => [normalizedSpeakerName(role.speaker), role.role]),
  ), [roles]);
  const roleFor = (participant: string) => {
    const speaker = speakers.find(
      ({ display_name }) => normalizedSpeakerName(display_name) === normalizedSpeakerName(participant),
    );
    return speaker ? roleForSpeaker(speaker, participantRoles) : undefined;
  };
  const openParticipantRenameDialog = useCallback((participant: string) => {
    if (!onRenameSpeaker) return;
    const speaker = speakers.find(
      ({ display_name }) => normalizedSpeakerName(display_name) === normalizedSpeakerName(participant),
    );
    if (speaker) setRenamingSpeaker(speaker);
  }, [onRenameSpeaker, speakers]);
  const linkedAgreements = useMemo(
    () => linkAgreementSpeakers(content.agreements, speakers, t),
    [content.agreements, speakers, t],
  );
  const linkedLead = useMemo(
    () => linkSpeakerNames(content.lead, speakers, t),
    [content.lead, speakers, t],
  );
  return (
    <div className={isSummaryLoading ? 'flex min-h-full flex-1 flex-col' : undefined}>
      {isSummaryLoading ? (
        <div
          className="flex min-h-0 flex-1 items-center justify-center"
          role="status"
          aria-label={isGenerating ? p.getSummaryStatusMessage(p.summaryStatus) : t('Loading saved summary...')}
        >
          <FluidSpinner className="h-10 w-10 text-[var(--primary-10)]" />
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
                        as="div"
                        start={<ParticipantAvatar userId={0} name={participants[0]} />}
                      >
                        <CellText
                          title={(
                            <RenameHint onClick={() => openParticipantRenameDialog(participants[0])}>
                              {participants[0]}
                            </RenameHint>
                          )}
                          description={roleFor(participants[0])}
                        />
                      </Cell>
                    </CellStack.Morph>
                    {participants.slice(1).map((participant, index) => (
                      <Cell
                        key={participant}
                        as="div"
                        start={<ParticipantAvatar userId={index + 1} name={participant} />}
                      >
                        <CellText
                          title={(
                            <RenameHint onClick={() => openParticipantRenameDialog(participant)}>
                              {participant}
                            </RenameHint>
                          )}
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
              <ChatMarkdown
                content={linkedLead}
                className="mm-summary-plain mt-2"
                components={{
                  a: ({ href, children }) => {
                    const speakerIdMatch = href?.match(/^#speaker-(\d+)$/u);
                    if (!speakerIdMatch || !onRenameSpeaker) {
                      return <a href={href}>{children}</a>;
                    }
                    const speaker = speakers.find(({ id }) => id === Number(speakerIdMatch[1]));
                    if (!speaker) return <>{children}</>;

                    return (
                      <Tooltip>
                        <TooltipTrigger asChild>
                          <button
                            type="button"
                            className="speaker-rename-underline text-left focus:outline-none focus-visible:ring-2 focus-visible:ring-[var(--primary-20)]"
                            onClick={() => setRenamingSpeaker(speaker)}
                          >
                            {children}
                          </button>
                        </TooltipTrigger>
                        <TooltipContent>{t('Rename')}</TooltipContent>
                      </Tooltip>
                    );
                  },
                }}
              />
            </section>
          ) : null}
          {content.agreements ? (
            <section>
              <h2 className="text-sm font-normal leading-5 text-[var(--primary-50)]">{t('Agreements')}</h2>
              <ChatMarkdown
                content={linkedAgreements}
                className="mm-summary-list mm-summary-plain mt-2"
                components={{
                  a: ({ href, children }) => {
                    const speakerIdMatch = href?.match(/^#speaker-(\d+)$/u);
                    if (!speakerIdMatch || !onRenameSpeaker) {
                      return <a href={href}>{children}</a>;
                    }
                    const speaker = speakers.find(({ id }) => id === Number(speakerIdMatch[1]));
                    if (!speaker) return <>{children}</>;

                    return (
                      <Tooltip>
                        <TooltipTrigger asChild>
                          <button
                            type="button"
                            className="speaker-rename-underline text-left focus:outline-none focus-visible:ring-2 focus-visible:ring-[var(--primary-20)]"
                            onClick={() => setRenamingSpeaker(speaker)}
                          >
                            {children}
                          </button>
                        </TooltipTrigger>
                        <TooltipContent>{t('Rename')}</TooltipContent>
                      </Tooltip>
                    );
                  },
                }}
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

      {onRenameSpeaker && renamingSpeaker && (
        <SpeakerRenameDialog
          open={true}
          currentName={renamingSpeaker.display_name}
          onOpenChange={(open) => { if (!open) setRenamingSpeaker(null); }}
          onRename={(name) => onRenameSpeaker(renamingSpeaker.id, name)}
        />
      )}
    </div>
  );
}
