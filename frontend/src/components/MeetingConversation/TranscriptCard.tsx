"use client";

import { useId, useMemo } from 'react';
import { AnimatePresence, motion } from 'framer-motion';
import { Transcript, TranscriptSegmentData } from '@/types';
import { TranscriptPanel } from '@/components/MeetingDetails/TranscriptPanel';
import { DetectSpeakersButton } from '@/components/MeetingDetails/DetectSpeakersButton';
import { useT } from '@/lib/i18n';

/**
 * Transcript pin for the meeting conversation (variant 3a): a minimal rounded chip
 * in the feed — icon · "Транскрипт" · "N реплик · MM мин" · chevron — that expands
 * in place (no card / border) into a bounded, scrollable region with a compact
 * audio player and the timecoded segment list. Reuses `TranscriptPanel` (real
 * player / seek / pagination); its own toolbar and summary context field are hidden.
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
    <div>
      {/* Minimal chip + speaker detection */}
      <div className="flex flex-wrap items-center gap-2">
        <button
          type="button"
          onClick={() => onToggle(!expanded)}
          aria-expanded={expanded}
          aria-controls={panelId}
          className="inline-flex items-center gap-[9px] rounded-full bg-[var(--bg-elevated)] py-2 pl-3 pr-3.5 text-[var(--fg2)] transition-colors hover:text-[var(--fg1)]"
        >
          <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
            <path d="M5 7h14M5 12h14M5 17h8" />
          </svg>
          <span className="text-[13px] font-semibold">{t('Transcript')}</span>
          {meta && <span className="mm-numeric text-[11.5px] text-[var(--fg3)]">{meta}</span>}
          <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="var(--fg3)" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <path d={expanded ? 'm6 15 6-6 6 6' : 'm6 9 6 6 6-6'} />
          </svg>
        </button>

        {/* Run speaker diarization on this meeting's transcript. */}
        <DetectSpeakersButton
          meetingId={meetingId}
          speakerCount={speakerCount}
          onDetected={onSpeakersDetected}
          label={t('Highlight speakers')}
        />
      </div>

      <AnimatePresence initial={false}>
        {expanded && (
          <motion.div
            id={panelId}
            initial={{ height: 0, opacity: 0 }}
            animate={{ height: 'auto', opacity: 1 }}
            exit={{ height: 0, opacity: 0 }}
            transition={{ duration: 0.2, ease: 'easeOut' }}
            className="overflow-hidden"
          >
            {/* Bounded height so the transcript never crowds out the summary and chat. */}
            <div className="mt-3 flex h-[min(46vh,440px)] flex-col">
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
