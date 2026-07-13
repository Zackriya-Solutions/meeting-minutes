"use client";

import { Transcript, TranscriptSegmentData } from '@/types';
import { TranscriptView } from '@/components/TranscriptView';
import { VirtualizedTranscriptView } from '@/components/VirtualizedTranscriptView';
import { TranscriptButtonGroup } from './TranscriptButtonGroup';
import { Icon } from '@/components/memento/Icon';
import { useMemo } from 'react';

function formatClock(totalSeconds: number): string {
  const mins = Math.floor(totalSeconds / 60);
  const secs = Math.floor(totalSeconds % 60);
  return `${mins}:${String(secs).padStart(2, '0')}`;
}

interface TranscriptPanelProps {
  transcripts: Transcript[];
  customPrompt: string;
  onPromptChange: (value: string) => void;
  onCopyTranscript: () => void;
  onOpenMeetingFolder: () => Promise<void>;
  isRecording: boolean;
  disableAutoScroll?: boolean;

  // Optional pagination props (when using virtualization)
  usePagination?: boolean;
  segments?: TranscriptSegmentData[];
  hasMore?: boolean;
  isLoadingMore?: boolean;
  totalCount?: number;
  loadedCount?: number;
  onLoadMore?: () => void;

  // Retranscription props
  meetingId?: string;
  meetingFolderPath?: string | null;
  onRefetchTranscripts?: () => Promise<void>;

  /** Jump-to-timestamp (seconds) from search results / RAG citations. */
  scrollToTimestamp?: number | null;

  /** Marked moments (elapsed seconds) captured during recording. */
  markedMoments?: number[];
  /** Jump the transcript to a marked moment. */
  onSeekToMoment?: (seconds: number) => void;

  // Speaker diarization props
  speakersById?: Map<number, string> | null;
  onRenameSpeaker?: (speakerId: number, displayName: string) => Promise<void> | void;
  /** Refresh speakers + transcripts after a successful detect. */
  onSpeakersDetected?: () => Promise<void> | void;
}

export function TranscriptPanel({
  transcripts,
  customPrompt,
  onPromptChange,
  onCopyTranscript,
  onOpenMeetingFolder,
  isRecording,
  disableAutoScroll = false,
  usePagination = false,
  segments,
  hasMore,
  isLoadingMore,
  totalCount,
  loadedCount,
  onLoadMore,
  meetingId,
  meetingFolderPath,
  onRefetchTranscripts,
  scrollToTimestamp = null,
  markedMoments = [],
  onSeekToMoment,
  speakersById = null,
  onRenameSpeaker,
  onSpeakersDetected,
}: TranscriptPanelProps) {
  // Convert transcripts to segments if pagination is not used but we want virtualization
  const convertedSegments = useMemo(() => {
    if (usePagination && segments) {
      return segments;
    }
    // Convert transcripts to segments for virtualization
    return transcripts.map(t => ({
      id: t.id,
      timestamp: t.audio_start_time ?? 0,
      endTime: t.audio_end_time,
      text: t.text,
      confidence: t.confidence,
      speaker: t.speaker,
      speaker_id: t.speaker_id,
    }));
  }, [transcripts, usePagination, segments]);

  return (
    <div className="relative hidden min-w-0 shrink-0 flex-col border-r border-[var(--border-subtle)] bg-[var(--bg-sheet)] md:flex md:w-1/4 lg:w-1/3">
      {/* Title area */}
      <div className="border-b border-[var(--border-subtle)] p-4">
        <TranscriptButtonGroup
          transcriptCount={usePagination ? (totalCount ?? convertedSegments.length) : (transcripts?.length || 0)}
          onCopyTranscript={onCopyTranscript}
          onOpenMeetingFolder={onOpenMeetingFolder}
          meetingId={meetingId}
          meetingFolderPath={meetingFolderPath}
          onRefetchTranscripts={onRefetchTranscripts}
          onSpeakersDetected={onSpeakersDetected}
        />
      </div>

      {/* Marked moments — click to jump the transcript to that point */}
      {markedMoments.length > 0 && (
        <div className="flex flex-wrap items-center gap-1.5 border-b border-[var(--border-subtle)] px-4 py-2">
          <span className="mm-eyebrow mr-1">Моменты</span>
          {markedMoments.map((seconds) => (
            <button
              key={seconds}
              type="button"
              onClick={() => onSeekToMoment?.(seconds)}
              title="Открыть этот момент в расшифровке"
              className="mm-numeric inline-flex items-center gap-1 rounded-full border border-[var(--gold-border)] bg-[var(--gold-soft)] px-2 py-0.5 text-xs text-[var(--gold)] transition-colors hover:bg-[var(--gold-soft-strong)]"
            >
              <Icon name="dot" size={12} />
              {formatClock(seconds)}
            </button>
          ))}
        </div>
      )}

      {/* Transcript content - use virtualized view for better performance */}
      <div className="flex-1 overflow-hidden pb-4">
        <VirtualizedTranscriptView
          segments={convertedSegments}
          isRecording={isRecording}
          isPaused={false}
          isProcessing={false}
          isStopping={false}
          enableStreaming={false}
          showConfidence={true}
          disableAutoScroll={disableAutoScroll}
          hasMore={hasMore}
          isLoadingMore={isLoadingMore}
          totalCount={totalCount}
          loadedCount={loadedCount}
          onLoadMore={onLoadMore}
          scrollToTimestamp={scrollToTimestamp}
          speakersById={speakersById}
          onRenameSpeaker={onRenameSpeaker}
        />
      </div>

      {/* Custom prompt input at bottom of transcript section */}
      {!isRecording && convertedSegments.length > 0 && (
        <div className="border-t border-[var(--border-subtle)] p-3">
          <textarea
            placeholder="Add context for AI summary. For example people involved, meeting overview, objective etc..."
            className="mm-field min-h-[80px] w-full resize-y px-3 py-2 text-sm outline-none"
            value={customPrompt}
            onChange={(e) => onPromptChange(e.target.value)}
          />
        </div>
      )}
    </div>
  );
}
