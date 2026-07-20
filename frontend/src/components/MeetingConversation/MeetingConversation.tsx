"use client";

import { type ComponentProps, useCallback, useEffect, useMemo, useState } from 'react';
import { Loader2 } from '@/components/memento/LucideCompat';
import { Transcript, TranscriptSegmentData } from '@/types';
import { EditableTitle } from '@/components/EditableTitle';
import { SummaryPanel } from '@/components/MeetingDetails/SummaryPanel';
import { useMeetingChat, type Citation } from '@/hooks/useMeetingChat';
import { MessageBubble, TypingIndicator } from '@/components/chat/MessageBubble';
import { Icon } from '@/components/memento/Icon';
import { useT } from '@/lib/i18n';
import Analytics from '@/lib/analytics';
import { TranscriptCard } from './TranscriptCard';
import { SummaryMessage } from './SummaryMessage';
import { MeetingComposer } from './MeetingComposer';
import { MeetingOverflowMenu } from './MeetingOverflowMenu';

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

const transcriptExpandedKey = (meetingId: string) => `memento:conversation:transcript-expanded:${meetingId}`;

function readStoredExpanded(meetingId: string): boolean {
  if (typeof window === 'undefined') return false;
  try {
    return window.localStorage.getItem(transcriptExpandedKey(meetingId)) === '1';
  } catch {
    return false;
  }
}

interface MeetingConversationProps {
  meetingId: string;
  meeting: { id: string; title: string; created_at: string };

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
  const { messages, input, setInput, sending, loadingHistory, send, onKeyDown, scrollRef, inputRef } = chat;

  const [transcriptExpanded, setTranscriptExpanded] = useState(() => readStoredExpanded(meetingId));
  const [seekTarget, setSeekTarget] = useState<number | null>(seekToSeconds);

  // Re-read the persisted expand state (and drop a stale seek) when the meeting changes.
  useEffect(() => {
    setTranscriptExpanded(readStoredExpanded(meetingId));
    setSeekTarget(null);
  }, [meetingId]);

  // Honor the ?t= deep link / citation from another screen: expand + scroll.
  useEffect(() => {
    if (seekToSeconds == null) return;
    setSeekTarget(seekToSeconds);
    setTranscriptExpanded(true);
  }, [seekToSeconds]);

  // A speaker-identity review sample (bounded excerpt) opens and scrolls the pin
  // to the sample start; TranscriptPanel handles the bounded playback itself.
  useEffect(() => {
    if (!playbackRequest) return;
    setTranscriptExpanded(true);
    setSeekTarget(playbackRequest.startSeconds + Math.random() * 0.02);
  }, [playbackRequest?.requestId]);

  const setExpandedPersisted = useCallback(
    (next: boolean) => {
      setTranscriptExpanded(next);
      try {
        window.localStorage.setItem(transcriptExpandedKey(meetingId), next ? '1' : '0');
      } catch {
        /* ignore */
      }
      if (next) Analytics.trackFeatureUsed('conversation_transcript_expand');
    },
    [meetingId],
  );

  // A citation inside the meeting chat opens the transcript pin and scrolls to the
  // cited moment in place (a tiny jitter re-triggers the scroll for repeat clicks).
  const handleCite = useCallback(
    (c: Citation) => {
      Analytics.trackFeatureUsed('conversation_citation_click');
      setExpandedPersisted(true);
      setSeekTarget(c.start_ms / 1000 + Math.random() * 0.02);
    },
    [setExpandedPersisted],
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

  const hasTranscript = (totalCount ?? segments?.length ?? transcripts.length) > 0;

  return (
    <div className="flex h-screen flex-col bg-[var(--bg-canvas)]">
      {/* Top bar */}
      <div className="flex items-center gap-3 border-b border-[var(--border-subtle)] px-5 py-3">
        <div className="min-w-0 flex-1">
          <EditableTitle
            title={meetingTitle}
            isEditing={isEditingTitle}
            onStartEditing={onStartEditTitle}
            onFinishEditing={onFinishEditTitle}
            onChange={onTitleChange}
          />
          {metaLine && <p className="mm-numeric mt-0.5 truncate text-xs text-[var(--fg3)]">{metaLine}</p>}
        </div>
        <MeetingOverflowMenu
          meetingId={meetingId}
          meetingFolderPath={meetingFolderPath}
          hasSummary={hasSummary}
          isSaving={isSaving}
          isDirty={isSummaryDirty}
          onCopySummary={onCopySummary}
          onSaveSummary={onSaveSummary}
          onOpenModelSettings={onOpenModelSettings}
          speakerCount={speakerCount}
          onSpeakersDetected={onSpeakersDetected}
          onRefetchTranscripts={onRefetchTranscripts}
        />
      </div>

      {/* Thread */}
      <div ref={scrollRef} className="flex-1 overflow-y-auto px-5 py-6">
        <div className="mx-auto flex max-w-3xl flex-col gap-5">
          {/* Pin #1 — transcript */}
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
            expanded={transcriptExpanded}
            onToggle={setExpandedPersisted}
          />

          {/* Pin #2 — summary as the first assistant message */}
          <SummaryMessage summaryPanelProps={summaryProps} />

          {/* Chat thread (meeting-scoped RAG session) */}
          {loadingHistory ? (
            <div className="flex items-center justify-center py-6 text-[var(--fg3)]">
              <Loader2 className="h-5 w-5 animate-spin" />
            </div>
          ) : (
            <>
              {messages.length === 0 && (
                <div className="flex items-center gap-2 rounded-[var(--radius-12)] border border-dashed border-[var(--border-subtle)] px-4 py-3 text-xs text-[var(--fg3)]">
                  <Icon name="chat" size={14} />
                  {t('Ask a question below to discuss this meeting.')}
                </div>
              )}
              {messages.map((msg, i) => (
                <MessageBubble
                  key={i}
                  msg={msg}
                  meetingTitle={meetingTitleFor}
                  onCite={handleCite}
                  showMeetingLabel={false}
                />
              ))}
              {sending && <TypingIndicator />}
            </>
          )}
        </div>
      </div>

      {/* Composer */}
      <div className="px-5">
        <MeetingComposer
          input={input}
          onInputChange={setInput}
          onKeyDown={onKeyDown}
          onSend={send}
          sending={sending}
          disabled={loadingHistory || !hasTranscript}
          inputRef={inputRef}
          suggestions={MEETING_SUGGESTIONS}
          showSuggestions={messages.length === 0}
        />
      </div>
    </div>
  );
}
