"use client";

import { useId, useMemo } from 'react';
import { AnimatePresence, motion } from 'framer-motion';
import { Transcript, TranscriptSegmentData } from '@/types';
import { TranscriptPanel } from '@/components/MeetingDetails/TranscriptPanel';
import { Icon } from '@/components/memento/Icon';
import { useT } from '@/lib/i18n';

/**
 * Transcript pin for the meeting conversation (variant 2a). Collapsed by default:
 * a single header row (icon · "Transcript" · "N replies · MM min" · chevron) that
 * expands in place — not a slide-out panel — into a bounded, scrollable region
 * with a compact audio player. Reuses `TranscriptPanel` for the segment list,
 * seek-to-timestamp, and correction behavior; its own toolbar and summary context
 * field are hidden (those actions live in the screen's "⋯" menu).
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
  /** Jump-to-timestamp (seconds) — driven by ?t= deep links and citation clicks. */
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

function formatMinutes(seconds: number, t: (en: string) => string): string {
  const mins = Math.max(1, Math.round(seconds / 60));
  return `${mins} ${t('min')}`;
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
    durationSeconds > 0 ? formatMinutes(durationSeconds, t) : null,
  ]
    .filter(Boolean)
    .join(' · ');

  return (
    <div className="overflow-hidden rounded-[var(--radius-16)] border border-[var(--border-subtle)] bg-[var(--bg-sheet)]">
      <button
        type="button"
        onClick={() => onToggle(!expanded)}
        aria-expanded={expanded}
        aria-controls={panelId}
        className="mm-hover flex w-full items-center gap-3 px-4 py-3 text-left"
      >
        <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-full bg-[var(--gold-soft)] text-[var(--gold)]">
          <Icon name="transcript" size={16} />
        </span>
        <span className="min-w-0 flex-1">
          <span className="block text-sm font-semibold text-[var(--fg1)]">{t('Transcript')}</span>
          {meta && <span className="mm-numeric block text-xs text-[var(--fg3)]">{meta}</span>}
        </span>
        <Icon
          name={expanded ? 'chevron-up' : 'chevron-down'}
          size={18}
          className="shrink-0 text-[var(--fg3)]"
        />
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
            <div className="flex h-[min(52vh,520px)] flex-col">
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
