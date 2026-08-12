"use client";

import { type ComponentProps, type UIEvent, useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Loader2, RefreshCw } from '@/components/deslop-icons';
import { SpeakerInfo, Transcript, TranscriptSegmentData } from '@/types';
import { EditableTitle } from '@/components/EditableTitle';
import { SummaryPanel } from '@/components/MeetingDetails/SummaryPanel';
import { useMeetingChat, type Citation } from '@/hooks/useMeetingChat';
import { useMeetingRefinement } from '@/hooks/useMeetingRefinement';
import { useAnalyticsReport } from '@/hooks/meeting-details/useAnalyticsReport';
import { useMeetingAnalyticsSections } from '@/hooks/meeting-details/useMeetingAnalyticsSections';
import { MeetingDynamicsPanel } from './analytics/MeetingDynamicsPanel';
import { MeetingNumbersPanel } from './analytics/MeetingNumbersPanel';
import { MeetingScoreSections } from './analytics/MeetingScoreSections';
import { MeetingTimelinePanel } from './analytics/MeetingTimelinePanel';
import { MessageBubble, TypingIndicator } from '@/components/chat/MessageBubble';
import { useLanguage } from '@/lib/i18n';
import Analytics from '@/lib/analytics';
import { readMeetingCalendarSchedule } from '@/lib/meetingCalendarSchedule';
import { TranscriptCard } from './TranscriptCard';
import { SummaryMessage } from './SummaryMessage';
import { MeetingComposer } from './MeetingComposer';
import { MeetingOverflowMenu } from './MeetingOverflowMenu';
import { TranscriptSearchDialog } from './TranscriptSearchDialog';
import { buildMeetingPromptSuggestions } from '@/lib/promptSuggestions';
import {
  FluidTabs,
  FluidTabsContent,
  FluidTabsList,
  FluidTabsTrigger,
} from '@/components/ui/fluid-tabs';
import { Button as FluidButton } from '@/components/ui/fluid-button';
import {
  MessageScroller,
  MessageScrollerButton,
  MessageScrollerContent,
  MessageScrollerItem,
  MessageScrollerProvider,
  MessageScrollerViewport,
  useMessageScrollerScrollable,
} from '@/components/ui/message-scroller';

type ScrollEdges = { start: boolean; end: boolean };

function ScrollEdgesObserver({ onChange }: { onChange: (edges: ScrollEdges) => void }) {
  const edges = useMessageScrollerScrollable();

  useEffect(() => {
    onChange({ start: edges.start, end: edges.end });
  }, [edges.end, edges.start, onChange]);

  return null;
}

/**
 * Variant 2a — the meeting screen as a single vertical conversation.
 *
 *   MeetingConversation
 *   ├─ TopBar          editable title · metadata · "⋯"
 *   ├─ Thread (scroll)
 *   │   ├─ TranscriptCard      [pin] collapsed by default
 *   │   ├─ SummaryMessage      [pin] assistant message #0 + type selector
 *   │   └─ ChatMessage[]       from useMeetingChat (meeting-scoped RAG session)
 *   └─ MeetingComposer suggestion chips + input → send()
 *
 * The pins are not part of the RAG session; they stay at the top and the thread
 * scrolls as a whole. Clicking a citation expands the transcript pin and scrolls
 * it to the cited segment (by start_ms) in place, instead of routing away.
 *
 * Three tabs: the summary with its analytical blocks and timeline, the transcript, and the
 * meeting chat. All report material comes from one completed run (see
 * `useMeetingAnalyticsSections`), so nothing here re-analyses a meeting; moment links jump
 * into the transcript tab, and «Открыть отчёт» opens that run's full HTML.
 */

type MeetingTab = 'summary' | 'transcript' | 'chat';

function formatMeetingDuration(seconds: number, language: string): string {
  const roundedMinutes = Math.max(1, Math.round(seconds / 60));
  const hours = Math.floor(roundedMinutes / 60);
  const minutes = roundedMinutes % 60;

  if (language === 'ru') {
    if (hours === 0) return `${minutes} мин`;
    return minutes > 0 ? `${hours} ч ${minutes} мин` : `${hours} ч`;
  }

  if (hours === 0) return `${minutes} min`;
  return minutes > 0 ? `${hours} hr ${minutes} min` : `${hours} hr`;
}

const NO_SCROLL_EDGES: Record<MeetingTab, ScrollEdges> = {
  summary: { start: false, end: false },
  transcript: { start: false, end: false },
  chat: { start: false, end: false },
};

interface MeetingConversationProps {
  meetingId: string;
  meeting: {
    id: string;
    title: string;
    created_at: string;
    occurred_at?: string | null;
    duration_seconds?: number | null;
  };

  /** Optional element pinned under the top bar (e.g. the speaker-identity review
   *  panel). Rendered as a non-scrolling row so it never pushes the composer off. */
  reviewSlot?: React.ReactNode;

  // Top-bar title editing (reuses inline rename → api_save_meeting_title upstream).
  meetingTitle: string;
  onTitleChange: (title: string) => void;
  isEditingTitle: boolean;
  onStartEditTitle: () => void;
  onFinishEditTitle: () => void;

  // Summary rendered as the first assistant message.
  summaryPanelProps: ComponentProps<typeof SummaryPanel>;

  // Secondary actions surfaced in the "⋯" menu.
  hasSummary: boolean;
  isSaving: boolean;
  isSummaryDirty: boolean;
  onCopySummary: () => Promise<void> | void;
  onOpenModelSettings: () => void;

  // Transcript pin data.
  transcripts: Transcript[];
  segments?: TranscriptSegmentData[];
  hasMore?: boolean;
  isLoadingMore?: boolean;
  totalCount?: number;
  loadedCount?: number;
  onLoadMore?: () => void;
  meetingFolderPath?: string | null;
  onRefetchTranscripts?: () => Promise<void>;
  onOpenMeetingFolder: () => Promise<void>;
  seekToSeconds?: number | null;
  playbackRequest?: {
    requestId: number;
    meetingId: string;
    startSeconds: number;
    endSeconds: number;
  } | null;
  markedMoments?: number[];
  speakers?: SpeakerInfo[];
  speakersById?: Map<number, string> | null;
  selfSpeakerIds?: ReadonlySet<number> | null;
  speakerCount?: number;
  onRenameSpeaker?: (speakerId: number, displayName: string) => Promise<void> | void;
  onSetSelfSpeaker?: (speakerId: number, isSelf: boolean) => Promise<void> | void;
  onSpeakersDetected?: () => Promise<void> | void;
}

export function MeetingConversation({
  meetingId,
  meeting,
  reviewSlot,
  meetingTitle,
  onTitleChange,
  isEditingTitle,
  onStartEditTitle,
  onFinishEditTitle,
  summaryPanelProps,
  hasSummary,
  isSaving,
  isSummaryDirty,
  onCopySummary,
  onOpenModelSettings,
  transcripts,
  segments,
  hasMore,
  isLoadingMore,
  totalCount,
  loadedCount,
  onLoadMore,
  meetingFolderPath,
  onRefetchTranscripts,
  onOpenMeetingFolder,
  seekToSeconds = null,
  playbackRequest = null,
  markedMoments = [],
  speakers = [],
  speakersById = null,
  selfSpeakerIds = null,
  speakerCount = 0,
  onRenameSpeaker,
  onSetSelfSpeaker,
  onSpeakersDetected,
}: MeetingConversationProps) {
  const { t, lang } = useLanguage();
  const locale = lang === 'ru' ? 'ru-RU' : 'en-US';
  // Post-meeting reprocessing: reply splitting is minutes of background CPU with no other
  // trigger and, until now, no visible sign it was running.
  const refinement = useMeetingRefinement(meetingId, onRefetchTranscripts);
  const chat = useMeetingChat({ scope: 'meeting', collectionId: null, meetingId, enabled: true });
  const { messages, input, setInput, sending, loadingHistory, send, onKeyDown, inputRef } = chat;
  const meetingSuggestions = useMemo(
    () => buildMeetingPromptSuggestions({
      title: meetingTitle || meeting.title,
      summary: summaryPanelProps.aiSummary,
      transcripts,
    }, lang),
    [lang, meeting.title, meetingTitle, summaryPanelProps.aiSummary, transcripts],
  );
  const [seekTarget, setSeekTarget] = useState<number | null>(seekToSeconds);
  const [activeTab, setActiveTab] = useState<MeetingTab>(
    seekToSeconds == null ? 'summary' : 'transcript',
  );
  const summaryInputRef = useRef<HTMLTextAreaElement>(null);
  const [tabScrollEdges, setTabScrollEdges] = useState(NO_SCROLL_EDGES);

  // One report pipeline per meeting screen: the "⋯" menu drives it, and the persisted
  // analytical blocks at the end of the summary come from its last finished run.
  const report = useAnalyticsReport(meetingId);
  const analytics = useMeetingAnalyticsSections(meetingId);
  const hasTranscript = (totalCount ?? transcripts.length) > 0;
  const autoReportMeetingRef = useRef<string | null>(null);

  // Build every post-meeting block automatically as soon as a transcript is available.
  // Wait for both persisted states to hydrate first: otherwise the initial `idle` render
  // could regenerate a report that already exists. The Rust command also reuses an active
  // run, so Strict Mode and quick remounts cannot create duplicate pipelines.
  useEffect(() => {
    if (
      !hasTranscript
      || !report.hydrated
      || analytics.loading
      || analytics.sections
      || !['idle', 'failed'].includes(report.status)
      || autoReportMeetingRef.current === meetingId
    ) {
      return;
    }

    autoReportMeetingRef.current = meetingId;
    void report.generate({ autoDownload: false, automatic: true });
  }, [
    analytics.loading,
    analytics.sections,
    hasTranscript,
    meetingId,
    report.generate,
    report.hydrated,
    report.status,
  ]);

  useEffect(() => {
    autoReportMeetingRef.current = null;
  }, [meetingId]);
  const setSummaryScrollEdges = useCallback((edges: ScrollEdges) => {
    setTabScrollEdges((current) => (
      current.summary.start === edges.start && current.summary.end === edges.end
        ? current
        : { ...current, summary: edges }
    ));
  }, []);

  const setChatScrollEdges = useCallback((edges: ScrollEdges) => {
    setTabScrollEdges((current) => (
      current.chat.start === edges.start && current.chat.end === edges.end
        ? current
        : { ...current, chat: edges }
    ));
  }, []);

  const handleThreadScrollCapture = useCallback((event: UIEvent<HTMLDivElement>) => {
    const target = event.target as HTMLElement;
    if (target.dataset.slot !== 'message-scroller-viewport') return;

    const edges = {
      start: target.scrollTop > 1,
      end: target.scrollHeight - target.scrollTop - target.clientHeight > 1,
    };
    setTabScrollEdges((current) => (
      current[activeTab].start === edges.start && current[activeTab].end === edges.end
        ? current
        : { ...current, [activeTab]: edges }
    ));
  }, [activeTab]);

  // Drop a stale seek when the meeting changes.
  useEffect(() => {
    setSeekTarget(null);
    setActiveTab('summary');
    setTabScrollEdges(NO_SCROLL_EDGES);
  }, [meetingId]);

  // Honor the ?t= deep link / citation from another screen: expand + scroll.
  useEffect(() => {
    if (seekToSeconds == null) return;
    setSeekTarget(seekToSeconds);
    setActiveTab('transcript');
  }, [seekToSeconds]);

  // A speaker-identity review sample (bounded excerpt) opens and scrolls the pin
  // to the sample start; TranscriptPanel handles the bounded playback itself.
  useEffect(() => {
    if (!playbackRequest) return;
    setSeekTarget(playbackRequest.startSeconds + Math.random() * 0.02);
    setActiveTab('transcript');
  }, [playbackRequest?.requestId]);

  // A citation inside the meeting chat opens the transcript pin and scrolls to the
  // cited moment in place (a tiny jitter re-triggers the scroll for repeat clicks).
  const handleCite = useCallback(
    (c: Citation) => {
      Analytics.trackFeatureUsed('conversation_citation_click');
      setSeekTarget(c.start_ms / 1000 + Math.random() * 0.02);
      setActiveTab('transcript');
    },
    [],
  );

  // Marked-moment chips inside the transcript pin scroll the pin to that point.
  const handleSeekToMoment = useCallback((seconds: number) => {
    setSeekTarget(seconds + Math.random() * 0.02);
    setActiveTab('transcript');
  }, []);

  const handleTranscriptSearchSelect = useCallback((seconds: number) => {
    Analytics.trackFeatureUsed('transcript_search_result_click');
    setActiveTab('transcript');
    setSeekTarget(seconds + Math.random() * 0.02);
  }, []);

  const focusComposer = useCallback(() => {
    setActiveTab('chat');
    window.requestAnimationFrame(() => {
      inputRef.current?.focus();
      inputRef.current?.scrollIntoView({ block: 'nearest' });
    });
  }, [inputRef]);

  const sendFromSummary = useCallback((text: string) => {
    if (!text.trim() || sending || loadingHistory) return;
    setActiveTab('chat');
    void send(text);
  }, [loadingHistory, send, sending]);

  const handleSummaryComposerKeyDown = useCallback(
    (event: React.KeyboardEvent<HTMLTextAreaElement>) => {
      if (event.key === 'Enter' && !event.shiftKey) {
        event.preventDefault();
        sendFromSummary(input);
        return;
      }
      onKeyDown(event);
    },
    [input, onKeyDown, sendFromSummary],
  );

  // Route the summary's "Discuss" action to this meeting's Chat tab and focus its composer.
  const summaryProps = useMemo<ComponentProps<typeof SummaryPanel>>(
    () => ({ ...summaryPanelProps, onDiscussSummary: focusComposer }),
    [summaryPanelProps, focusComposer],
  );
  const showSummaryContent = hasSummary
    || summaryPanelProps.summaryLoadStatus === 'loading'
    || ['processing', 'summarizing', 'regenerating'].includes(summaryPanelProps.summaryStatus)
    // A run that produced nothing still has to be reportable — with a way to try again.
    || !!summaryPanelProps.summaryError
    || !!summaryPanelProps.summaryLoadError;

  const meetingTitleFor = useCallback(() => meetingTitle || meeting.title, [meetingTitle, meeting.title]);
  const latestUserMessageIndex = messages.reduce(
    (latestIndex, message, index) => message.role === 'user' ? index : latestIndex,
    -1,
  );

  const actualDurationSeconds = useMemo(() => {
    const durationSource = segments && segments.length > 0
      ? segments.map((s) => s.endTime ?? s.timestamp)
      : transcripts.map((tr) => tr.audio_end_time ?? tr.audio_start_time ?? 0);
    const transcriptDuration = durationSource.length ? Math.max(...durationSource) : 0;
    return meeting.duration_seconds && meeting.duration_seconds > 0
      ? meeting.duration_seconds
      : transcriptDuration;
  }, [meeting.duration_seconds, segments, transcripts]);

  const calendarSchedule = useMemo(() => {
    if (typeof window === 'undefined') return null;
    return readMeetingCalendarSchedule(window.localStorage, meetingId);
  }, [meetingId]);

  const metaLine = useMemo(() => {
    const startedAt = new Date(meeting.occurred_at || meeting.created_at);
    if (Number.isNaN(startedAt.getTime())) return '';

    const timeFormatter = new Intl.DateTimeFormat(locale, {
      hour: '2-digit',
      minute: '2-digit',
    });

    if (actualDurationSeconds <= 0) return timeFormatter.format(startedAt);
    const endedAt = new Date(startedAt.getTime() + actualDurationSeconds * 1_000);
    const metadata = [
      `${timeFormatter.format(startedAt)}\u00a0–\u00a0${timeFormatter.format(endedAt)}`,
      formatMeetingDuration(actualDurationSeconds, lang),
    ];

    if (calendarSchedule) {
      const plannedDurationSeconds = (
        new Date(calendarSchedule.endAt).getTime() - new Date(calendarSchedule.startAt).getTime()
      ) / 1_000;
      if (actualDurationSeconds > plannedDurationSeconds + 1) metadata.push(t('Ran over'));
    }

    return metadata.join(' · ');
  }, [actualDurationSeconds, calendarSchedule, lang, meeting.created_at, meeting.occurred_at, locale, t]);

  return (
    <div className="meeting-conversation meeting-page-surface flex h-full flex-col">
      {/* Top bar */}
      <div className="flex items-center gap-3 px-[var(--drawer-content-inset)] pb-3 pt-4">
        <div className="min-w-0 flex-1">
          <EditableTitle
            title={meetingTitle}
            isEditing={isEditingTitle}
            onStartEditing={onStartEditTitle}
            onFinishEditing={onFinishEditTitle}
            onChange={onTitleChange}
            showEditButton={false}
            seamlessEditing
          />
          {metaLine && <p className="mm-numeric mt-0.5 truncate text-xs text-muted-foreground">{metaLine}</p>}
        </div>
        <div className="flex shrink-0 items-center gap-1.5">
          <TranscriptSearchDialog
            meetingId={meetingId}
            transcripts={transcripts}
            totalCount={totalCount}
            onSelect={handleTranscriptSearchSelect}
          />
          {hasSummary && (
            <FluidButton
              type="button"
              variant="secondary"
              size="icon"
              aria-label={t('Regenerate')}
              title={t('Regenerate')}
              data-no-window-drag
              disabled={['processing', 'summarizing', 'regenerating'].includes(summaryPanelProps.summaryStatus)}
              onClick={() => void summaryPanelProps.onRegenerateSummary()}
              className="no-drag h-10 w-10 rounded-full shadow-none [&>span:first-child]:!bg-[var(--primary-5)] active:scale-[0.96]"
            >
              <RefreshCw size={18} strokeWidth={2} />
            </FluidButton>
          )}
          <MeetingOverflowMenu
            meetingId={meetingId}
            report={report}
            canOpenReport={!!analytics.sections}
            hasSummary={hasSummary}
            hasTranscript={hasTranscript}
            onCopySummary={onCopySummary}
            onRenameMeeting={onStartEditTitle}
            onShareSummaryToTelegram={summaryPanelProps.onShareSummaryToTelegram}
            canShareToTelegram={summaryPanelProps.canShareToTelegram}
            modelConfig={summaryPanelProps.modelConfig}
            setModelConfig={summaryPanelProps.setModelConfig}
            onSaveModelConfig={summaryPanelProps.onSaveModelConfig}
            onReprocess={refinement.rerun}
            reprocessingLabel={refinement.label}
          />
        </div>
      </div>

      {reviewSlot && <div className="shrink-0">{reviewSlot}</div>}

      <FluidTabs
        value={activeTab}
        onValueChange={(value) => setActiveTab(value as MeetingTab)}
        onScrollCapture={handleThreadScrollCapture}
        className="flex min-h-0 flex-1 flex-col"
      >
        <FluidTabsList className="mx-[var(--drawer-content-inset)] mb-3 h-auto min-h-10 w-auto shrink-0 flex-wrap">
          <FluidTabsTrigger value="summary" className="whitespace-nowrap px-2.5 shadow-none">
            {t('Summary tab')}
          </FluidTabsTrigger>
          <FluidTabsTrigger value="chat" className="whitespace-nowrap px-2.5 shadow-none">
            {t('Chat tab')}
          </FluidTabsTrigger>
          <FluidTabsTrigger value="transcript" className="whitespace-nowrap px-2.5 shadow-none">
            {t('Transcript tab')}
          </FluidTabsTrigger>
        </FluidTabsList>

        <div
          aria-hidden="true"
          className={`h-px w-full shrink-0 bg-[var(--primary-10)] transition-opacity duration-150 ${
            tabScrollEdges[activeTab].start ? 'opacity-100' : 'opacity-0'
          }`}
        />

        <FluidTabsContent value="summary" className="mt-0 min-h-0 flex-1 data-[state=active]:flex data-[state=active]:flex-col data-[state=inactive]:hidden">
          <MessageScrollerProvider
            key={meetingId}
            autoScroll={false}
            defaultScrollPosition="start"
            scrollPreviousItemPeek={48}
          >
            <ScrollEdgesObserver onChange={setSummaryScrollEdges} />
            <div className="relative flex min-h-0 flex-1 flex-col">
              <MessageScroller className="min-h-0 flex-1">
                <MessageScrollerViewport className="scroll-pb-28 px-[var(--drawer-content-inset)] pb-28 pt-1.5">
                  <MessageScrollerContent className="mx-auto max-w-[720px] gap-6 pb-4">
                    {showSummaryContent && (
                      <MessageScrollerItem messageId={`${meetingId}-summary`}>
                        <SummaryMessage
                          summaryPanelProps={summaryProps}
                          speakers={speakers}
                          roles={analytics.sections?.roles}
                        />
                      </MessageScrollerItem>
                    )}

                    {analytics.sections && (
                      <MessageScrollerItem messageId={`${meetingId}-hindrances`}>
                        <MeetingScoreSections
                          sections={analytics.sections}
                        />
                      </MessageScrollerItem>
                    )}

                    {analytics.sections && (
                      <>
                        <MessageScrollerItem messageId={`${meetingId}-numbers`}>
                          <MeetingNumbersPanel
                            numbers={analytics.sections.numbers}
                            onSeek={handleSeekToMoment}
                          />
                        </MessageScrollerItem>
                        <MessageScrollerItem messageId={`${meetingId}-dynamics`}>
                          <MeetingDynamicsPanel
                            dynamics={analytics.sections.dynamics}
                          />
                        </MessageScrollerItem>
                        <MessageScrollerItem messageId={`${meetingId}-timeline`}>
                          <MeetingTimelinePanel
                            timeline={analytics.sections.timeline}
                            onSeek={handleSeekToMoment}
                          />
                        </MessageScrollerItem>
                      </>
                    )}
                  </MessageScrollerContent>
                </MessageScrollerViewport>
                <MessageScrollerButton className="data-[direction=end]:bottom-[92px]" />
              </MessageScroller>

              <MeetingComposer
                input={input}
                onInputChange={setInput}
                onKeyDown={handleSummaryComposerKeyDown}
                onSend={sendFromSummary}
                sending={sending}
                disabled={loadingHistory}
                inputRef={summaryInputRef}
                suggestions={meetingSuggestions}
              />
            </div>
          </MessageScrollerProvider>
        </FluidTabsContent>

        <FluidTabsContent value="transcript" className="mt-0 min-h-0 flex-1 overflow-hidden data-[state=inactive]:hidden">
          <TranscriptCard
            meetingId={meetingId}
            meetingFolderPath={meetingFolderPath}
            transcripts={transcripts}
            segments={segments}
            hasMore={hasMore}
            isLoadingMore={isLoadingMore}
            totalCount={totalCount}
            loadedCount={loadedCount}
            onLoadMore={onLoadMore}
            onRefetchTranscripts={onRefetchTranscripts}
            onOpenMeetingFolder={onOpenMeetingFolder}
            scrollToTimestamp={seekTarget}
            playbackRequest={playbackRequest}
            markedMoments={markedMoments}
            onSeekToMoment={handleSeekToMoment}
            speakersById={speakersById}
            selfSpeakerIds={selfSpeakerIds}
            speakerCount={speakerCount}
            onRenameSpeaker={onRenameSpeaker}
            onSetSelfSpeaker={onSetSelfSpeaker}
            onSpeakersDetected={onSpeakersDetected}
            transcriptViewportClassName="px-[var(--drawer-content-inset)]"
          />
        </FluidTabsContent>

        <FluidTabsContent value="chat" className="mt-0 min-h-0 flex-1 data-[state=active]:flex data-[state=active]:flex-col data-[state=inactive]:hidden">
          <MessageScrollerProvider
            key={`${meetingId}-chat`}
            autoScroll
            defaultScrollPosition="end"
            scrollPreviousItemPeek={48}
          >
            <ScrollEdgesObserver onChange={setChatScrollEdges} />
            <div className="relative flex min-h-0 flex-1 flex-col">
              <MessageScroller className="min-h-0 flex-1">
                <MessageScrollerViewport className="scroll-pb-28 px-[var(--drawer-content-inset)] pb-28 pt-1.5">
                  <MessageScrollerContent className="mx-auto max-w-[720px] gap-6 pb-4">
                    {loadingHistory ? (
                      <MessageScrollerItem>
                        <div className="flex items-center justify-center py-6 text-muted-foreground">
                          <Loader2 className="h-5 w-5 animate-spin" />
                        </div>
                      </MessageScrollerItem>
                    ) : (
                      <>
                        {messages.map((msg, i) => (
                          <MessageScrollerItem
                            key={`${msg.role}-${i}`}
                            messageId={`${meetingId}-chat-${i}`}
                            scrollAnchor={i === latestUserMessageIndex}
                          >
                            <MessageBubble
                              msg={msg}
                              meetingTitle={meetingTitleFor}
                              onCite={handleCite}
                              showMeetingLabel={false}
                            />
                          </MessageScrollerItem>
                        ))}
                        {sending && (
                          <MessageScrollerItem messageId={`${meetingId}-chat-typing`}>
                            <TypingIndicator />
                          </MessageScrollerItem>
                        )}
                      </>
                    )}
                  </MessageScrollerContent>
                </MessageScrollerViewport>
                <MessageScrollerButton className="data-[direction=end]:bottom-[92px]" />
              </MessageScroller>

              <MeetingComposer
                input={input}
                onInputChange={setInput}
                onKeyDown={onKeyDown}
                onSend={send}
                sending={sending}
                disabled={loadingHistory}
                inputRef={inputRef}
                suggestions={meetingSuggestions}
              />
            </div>
          </MessageScrollerProvider>
        </FluidTabsContent>

      </FluidTabs>
    </div>
  );
}
