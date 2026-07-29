"use client";

import { Transcript, TranscriptSegmentData } from '@/types';
import { TranscriptPanel } from '@/components/MeetingDetails/TranscriptPanel';

/**
 * Transcript pin for the meeting conversation. The transcript stays visible as a
 * bounded, scrollable region without a separate toolbar or disclosure controls.
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
}: TranscriptCardProps) {
  return (
    <div className="flex h-[min(46vh,440px)] flex-col">
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
  );
}
