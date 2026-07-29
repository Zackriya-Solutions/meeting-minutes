"use client";

import { type ComponentProps, useCallback, useEffect, useMemo, useState } from 'react';
import { Loader2 } from '@/components/deslop-icons';
import { Transcript, TranscriptSegmentData } from '@/types';
import { EditableTitle } from '@/components/EditableTitle';
import { SummaryPanel } from '@/components/MeetingDetails/SummaryPanel';
import { useMeetingChat, type Citation } from '@/hooks/useMeetingChat';
import { MessageBubble, TypingIndicator } from '@/components/chat/MessageBubble';
import { useT } from '@/lib/i18n';
import Analytics from '@/lib/analytics';
import { TranscriptCard } from './TranscriptCard';
import { SummaryMessage } from './SummaryMessage';
import { MeetingComposer } from './MeetingComposer';
import { MeetingOverflowMenu } from './MeetingOverflowMenu';
import { AnalyticsReportButton } from './AnalyticsReportButton';
import {
  MessageScroller,
  MessageScrollerButton,
  MessageScrollerContent,
  MessageScrollerItem,
  MessageScrollerProvider,
  MessageScrollerViewport,
} from '@/components/ui/message-scroller';

/**
 * Variant 2a — the meeting screen as a single vertical conversation.
 *
 *   MeetingConversation
 *   ├─ TopBar          editable title · metadata · "⋯"
 *   ├─ Thread (scroll)
 *   │   ├─ TranscriptCard   [pin] collapsed by default
 *   │   ├─ SummaryMessage   [pin] assistant message #0 + type selector
 *   │   └─ ChatMessage[]    from useMeetingChat (meeting-scoped RAG session)
 *   └─ MeetingComposer suggestion chips + input → send()
 *
 * The pins are not part of the RAG session; they stay at the top and the thread
 * scrolls as a whole. Clicking a citation expands the transcript pin and scrolls
 * it to the cited segment (by start_ms) in place, instead of routing away.
 */

const MEETING_SUGGESTIONS = [
  'What tasks are due this week?',
  'What are the risks to the deadlines?',
  'Make a list of action items.',
];

interface MeetingConversationProps {
  meetingId: string;
  meeting: { id: string; title: string; created_at: string };

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
  onSaveSummary: () => Promise<void> | void;
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
  speakersById?: Map<number, string> | null;
  speakerCount?: number;
  onRenameSpeaker?: (speakerId: number, displayName: string) => Promise<void> | void;
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
  onSaveSummary,
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
  speakersById = null,
  speakerCount = 0,
  onRenameSpeaker,
  onSpeakersDetected,
}: MeetingConversationProps) {
  const t = useT();
  const chat = useMeetingChat({ scope: 'meeting', collectionId: null, meetingId, enabled: true });
  const { messages, input, setInput, sending, loadingHistory, send, onKeyDown, inputRef } = chat;

  const [seekTarget, setSeekTarget] = useState<number | null>(seekToSeconds);

  // Drop a stale seek when the meeting changes.
  useEffect(() => {
    setSeekTarget(null);
  }, [meetingId]);

  // Honor the ?t= deep link / citation from another screen: expand + scroll.
  useEffect(() => {
    if (seekToSeconds == null) return;
    setSeekTarget(seekToSeconds);
  }, [seekToSeconds]);

  // A speaker-identity review sample (bounded excerpt) opens and scrolls the pin
  // to the sample start; TranscriptPanel handles the bounded playback itself.
  useEffect(() => {
    if (!playbackRequest) return;
    setSeekTarget(playbackRequest.startSeconds + Math.random() * 0.02);
  }, [playbackRequest?.requestId]);

  // A citation inside the meeting chat opens the transcript pin and scrolls to the
  // cited moment in place (a tiny jitter re-triggers the scroll for repeat clicks).
  const handleCite = useCallback(
    (c: Citation) => {
      Analytics.trackFeatureUsed('conversation_citation_click');
      setSeekTarget(c.start_ms / 1000 + Math.random() * 0.02);
    },
    [],
  );

  // Marked-moment chips inside the transcript pin scroll the pin to that point.
  const handleSeekToMoment = useCallback((seconds: number) => {
    setSeekTarget(seconds + Math.random() * 0.02);
  }, []);

  const focusComposer = useCallback(() => {
    inputRef.current?.focus();
    inputRef.current?.scrollIntoView({ block: 'nearest' });
  }, [inputRef]);

  // Route the summary's "Discuss" action to the inline composer instead of a
  // separate chat screen — the whole conversation now lives here.
  const summaryProps = useMemo<ComponentProps<typeof SummaryPanel>>(
    () => ({ ...summaryPanelProps, onDiscussSummary: focusComposer }),
    [summaryPanelProps, focusComposer],
  );

  const meetingTitleFor = useCallback(() => meetingTitle || meeting.title, [meetingTitle, meeting.title]);

  // The analytics report needs a transcript to work with. Derive availability from
  // the transcript props already flowing into this component (loaded rows, the total
  // count, or virtualized segments) — no extra plumbing required.
  const hasTranscript = (totalCount ?? 0) > 0 || transcripts.length > 0 || (segments?.length ?? 0) > 0;

  const metaLine = useMemo(() => {
    const parts: string[] = [];
    if (meeting.created_at) {
      const d = new Date(meeting.created_at);
      if (!Number.isNaN(d.getTime())) parts.push(d.toLocaleDateString());
    }
    const durationSource = segments && segments.length > 0
      ? segments.map((s) => s.endTime ?? s.timestamp)
      : transcripts.map((tr) => tr.audio_end_time ?? tr.audio_start_time ?? 0);
    const durationSeconds = durationSource.length ? Math.max(...durationSource) : 0;
    if (durationSeconds > 0) parts.push(`${Math.max(1, Math.round(durationSeconds / 60))} ${t('min')}`);
    if (speakerCount > 0) parts.push(`${speakerCount} ${t('participants')}`);
    return parts.join(' · ');
  }, [meeting.created_at, segments, transcripts, speakerCount, t]);

  return (
    <div className="flex h-full flex-col bg-[var(--elevation-1)]">
      {/* Top bar */}
      <div className="flex items-center gap-3 border-b border-border px-[22px] py-4">
        <div className="min-w-0 flex-1">
          <EditableTitle
            title={meetingTitle}
            isEditing={isEditingTitle}
            onStartEditing={onStartEditTitle}
            onFinishEditing={onFinishEditTitle}
            onChange={onTitleChange}
          />
          {metaLine && <p className="mm-numeric mt-0.5 truncate text-xs text-muted-foreground">{metaLine}</p>}
        </div>
        <AnalyticsReportButton
          meetingId={meetingId}
          disabled={!hasTranscript}
          disabledTitle={t('No transcript to analyze yet')}
        />
        <MeetingOverflowMenu
          meetingId={meetingId}
          hasSummary={hasSummary}
          onCopySummary={onCopySummary}
          onSaveSummary={onSaveSummary}
          modelConfig={summaryPanelProps.modelConfig}
          setModelConfig={summaryPanelProps.setModelConfig}
          onSaveModelConfig={summaryPanelProps.onSaveModelConfig}
        />
      </div>

      {reviewSlot && <div className="shrink-0">{reviewSlot}</div>}

      <MessageScrollerProvider
        key={`${meetingId}-${loadingHistory ? 'loading' : 'ready'}`}
        autoScroll
        defaultScrollPosition="last-anchor"
        scrollPreviousItemPeek={48}
      >
        <div className="flex min-h-0 flex-1 flex-col">
          {/* Thread */}
          <MessageScroller className="min-h-0 flex-1">
            <MessageScrollerViewport className="px-[26px] pb-2 pt-1.5">
              <MessageScrollerContent className="mx-auto max-w-[720px] gap-6">
                {/* Pin #1 — transcript */}
                <MessageScrollerItem messageId={`${meetingId}-transcript`}>
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
                    speakerCount={speakerCount}
                    onRenameSpeaker={onRenameSpeaker}
                    onSpeakersDetected={onSpeakersDetected}
                  />
                </MessageScrollerItem>

                {/* Pin #2 — summary as the first assistant message */}
                <MessageScrollerItem messageId={`${meetingId}-summary`}>
                  <SummaryMessage summaryPanelProps={summaryProps} />
                </MessageScrollerItem>

                {/* Divider before the chat thread */}
                <MessageScrollerItem aria-hidden="true">
                  <div className="h-px bg-border" />
                </MessageScrollerItem>

                {/* Chat thread (meeting-scoped RAG session) */}
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
                        scrollAnchor={msg.role === 'user'}
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
            <MessageScrollerButton />
          </MessageScroller>

          {/* Composer */}
          <MeetingComposer
            input={input}
            onInputChange={setInput}
            onKeyDown={onKeyDown}
            onSend={send}
            sending={sending}
            disabled={loadingHistory}
            inputRef={inputRef}
          />
        </div>
      </MessageScrollerProvider>
    </div>
  );
}
