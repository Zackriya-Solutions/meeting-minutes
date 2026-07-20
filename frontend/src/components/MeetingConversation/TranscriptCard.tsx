"use client";

import { useId, useMemo } from 'react';
import { AnimatePresence, motion } from 'framer-motion';
import { Transcript, TranscriptSegmentData } from '@/types';
import { TranscriptPanel } from '@/components/MeetingDetails/TranscriptPanel';
import { useT } from '@/lib/i18n';

/**
 * Transcript pin for the meeting conversation (variant 2a), matching the delivery
 * prototype: a `bg-surface` card whose header is a single clickable row —
 * "Транскрипт" · "N реплик · MM мин" · chevron — that expands in place (not a
 * slide-out) into a bounded, scrollable region with a compact audio player and the
 * timecoded segment list. Reuses `TranscriptPanel` (real player / seek / pagination
 * / correction); its own toolbar and summary context field are hidden.
 */

interface TranscriptCardProps {
  meetingId: string;
  meetingFolderPath?: string | null;
  transcripts: Transcript[];
  segments?: TranscriptSegmentData[];
  hasMore?: boolean;
  isLoadingMore?: boolean;
  totalCount?: number;
  loadedCount?: number;
  onLoadMore?: () => void;
  onRefetchTranscripts?: () => Promise<void>;
  onOpenMeetingFolder: () => Promise<void>;
  scrollToTimestamp?: number | null;
  playbackRequest?: {
    requestId: number;
    meetingId: string;
    startSeconds: number;
    endSeconds: number;
  } | null;
  markedMoments?: number[];
  onSeekToMoment?: (seconds: number) => void;
  speakersById?: Map<number, string> | null;
  speakerCount?: number;
  onRenameSpeaker?: (speakerId: number, displayName: string) => Promise<void> | void;
  onSpeakersDetected?: () => Promise<void> | void;
  expanded: boolean;
  onToggle: (expanded: boolean) => void;
}

export function TranscriptCard({
  meetingId,
  meetingFolderPath,
  transcripts,
  segments,
  hasMore,
  isLoadingMore,
  totalCount,
  loadedCount,
  onLoadMore,
  onRefetchTranscripts,
  onOpenMeetingFolder,
  scrollToTimestamp = null,
  playbackRequest = null,
  markedMoments = [],
  onSeekToMoment,
  speakersById = null,
  speakerCount = 0,
  onRenameSpeaker,
  onSpeakersDetected,
  expanded,
  onToggle,
}: TranscriptCardProps) {
  const t = useT();
  const panelId = useId();

  const replyCount = totalCount ?? segments?.length ?? transcripts.length;
  const durationSeconds = useMemo(() => {
    const source = segments && segments.length > 0
      ? segments.map((s) => s.endTime ?? s.timestamp)
      : transcripts.map((tr) => tr.audio_end_time ?? tr.audio_start_time ?? 0);
    return source.length ? Math.max(...source) : 0;
  }, [segments, transcripts]);

  const meta = [
    `${replyCount} ${t('replies')}`,
    durationSeconds > 0 ? `${Math.max(1, Math.round(durationSeconds / 60))} ${t('min')}` : null,
  ]
    .filter(Boolean)
    .join(' · ');

  return (
    <div className="overflow-hidden rounded-[16px] border border-[var(--border-subtle)] bg-[var(--bg-surface)]">
      <button
        type="button"
        onClick={() => onToggle(!expanded)}
        aria-expanded={expanded}
        aria-controls={panelId}
        className="flex w-full items-center gap-[11px] px-4 py-[13px] text-left transition-colors hover:bg-[var(--state-hover-bg)]"
      >
        <span className="min-w-0 flex-1">
          <span className="text-sm font-semibold text-[var(--fg1)]">{t('Transcript')}</span>
          {meta && <span className="mm-numeric ml-2 text-xs text-[var(--fg3)]">{meta}</span>}
        </span>
        <svg
          width="17"
          height="17"
          viewBox="0 0 24 24"
          fill="none"
          stroke="var(--fg3)"
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
          className="shrink-0"
        >
          <path d={expanded ? 'm6 15 6-6 6 6' : 'm6 9 6 6 6-6'} />
        </svg>
      </button>

      <AnimatePresence initial={false}>
        {expanded && (
          <motion.div
            id={panelId}
            initial={{ height: 0, opacity: 0 }}
            animate={{ height: 'auto', opacity: 1 }}
            exit={{ height: 0, opacity: 0 }}
            transition={{ duration: 0.2, ease: 'easeOut' }}
            className="overflow-hidden border-t border-[var(--border-subtle)]"
          >
            {/* Bounded height so the transcript never crowds out the summary and chat. */}
            <div className="flex h-[min(46vh,440px)] flex-col p-2">
              <TranscriptPanel
                transcripts={transcripts}
                customPrompt=""
                onPromptChange={() => {}}
                onCopyTranscript={() => {}}
                onOpenMeetingFolder={onOpenMeetingFolder}
                isRecording={false}
                disableAutoScroll
                usePagination
                segments={segments}
                hasMore={hasMore}
                isLoadingMore={isLoadingMore}
                totalCount={totalCount}
                loadedCount={loadedCount}
                onLoadMore={onLoadMore}
                meetingId={meetingId}
                meetingFolderPath={meetingFolderPath}
                onRefetchTranscripts={onRefetchTranscripts}
                scrollToTimestamp={scrollToTimestamp}
                playbackRequest={playbackRequest}
                markedMoments={markedMoments}
                onSeekToMoment={onSeekToMoment}
                speakersById={speakersById}
                speakerCount={speakerCount}
                onRenameSpeaker={onRenameSpeaker}
                onSpeakersDetected={onSpeakersDetected}
                showToolbar={false}
                showContextField={false}
                compactPlayer
              />
            </div>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}
