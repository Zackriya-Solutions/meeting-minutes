"use client";

import { Transcript, TranscriptSegmentData } from '@/types';
import { TranscriptView } from '@/components/TranscriptView';
import { VirtualizedTranscriptView } from '@/components/VirtualizedTranscriptView';
import { TranscriptButtonGroup } from './TranscriptButtonGroup';
import { useMemo } from 'react';

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
  mode?: 'transcript' | 'context';
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
  mode = 'transcript',
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
    }));
  }, [transcripts, usePagination, segments]);

  return (
    <section className="flex min-w-0 flex-1 flex-col bg-[var(--pt-bg)]" aria-label={mode === 'context' ? 'Meeting context' : 'Meeting transcript'}>
      <div className="mx-auto flex min-h-0 w-full max-w-[1180px] flex-1 flex-col px-5 py-5 md:px-8 md:py-6">
      {/* Title area */}
      <div className="mb-4 flex flex-col gap-1 border-b border-[var(--pt-border)] pb-4">
        <p className="pt-label text-[var(--pt-text-tertiary)]">{mode === 'context' ? 'Summary guidance' : 'Source record'}</p>
        <h2 className="text-[21px] font-medium tracking-[-.025em]">
          {mode === 'context' ? 'Context for this meeting' : 'Transcript'}
        </h2>
        <p className="text-sm text-[var(--pt-text-secondary)]">
          {mode === 'context'
            ? 'Add names, goals, terms, and background the summary should account for.'
            : 'Review the exact source, then copy or retranscribe when needed.'}
        </p>
      </div>

      {mode === 'transcript' && <div className="mb-4">
        <TranscriptButtonGroup
          transcriptCount={usePagination ? (totalCount ?? convertedSegments.length) : (transcripts?.length || 0)}
          onCopyTranscript={onCopyTranscript}
          onOpenMeetingFolder={onOpenMeetingFolder}
          meetingId={meetingId}
          meetingFolderPath={meetingFolderPath}
          onRefetchTranscripts={onRefetchTranscripts}
        />
      </div>}

      {/* Transcript content - use virtualized view for better performance */}
      {mode === 'transcript' && <div className="min-h-0 flex-1 overflow-hidden border border-[var(--pt-border)] bg-[var(--pt-surface)] pb-4 shadow-[0_1px_2px_rgba(11,11,12,.025)]">
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
        />
      </div>}

      {/* Custom prompt input at bottom of transcript section */}
      {mode === 'context' && !isRecording && convertedSegments.length > 0 && (
        <div className="max-w-3xl border-l-2 border-[var(--pt-accent)] bg-[var(--pt-surface)] p-5 shadow-[0_8px_20px_rgba(11,11,12,.07)]">
          <label htmlFor="meeting-summary-context" className="mb-2 block text-sm font-medium text-[var(--pt-text)]">
            What should pulse talq know?
          </label>
          <textarea
            id="meeting-summary-context"
            placeholder="Example: This is a product review with Sam and Priya. Focus on launch risks, decisions, and who owns each follow-up."
            className="pt-input min-h-[180px] resize-y py-3 font-[var(--pt-font-reading)] text-base leading-relaxed"
            value={customPrompt}
            onChange={(e) => onPromptChange(e.target.value)}
          />
          <p className="mt-3 text-xs text-[var(--pt-text-tertiary)]">
            pulse talq uses this context the next time you generate or regenerate outcomes.
          </p>
        </div>
      )}
      {mode === 'context' && convertedSegments.length === 0 && (
        <div className="border border-[var(--pt-border)] bg-[var(--pt-surface)] p-5 text-sm text-[var(--pt-text-secondary)]">
          Record or import audio before adding summary context.
        </div>
      )}
      </div>
    </section>
  );
}
