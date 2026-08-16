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
  selfSpeakerIds?: ReadonlySet<number> | null;
  speakerCount?: number;
  onRenameSpeaker?: (speakerId: number, displayName: string) => Promise<void> | void;
  onSetSelfSpeaker?: (speakerId: number, isSelf: boolean) => Promise<void> | void;
  onAssignSegmentSpeaker?: (transcriptId: string, speakerId: number) => Promise<void> | void;
  onSpeakersDetected?: () => Promise<void> | void;
  transcriptViewportClassName?: string;
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
  selfSpeakerIds = null,
  speakerCount = 0,
  onRenameSpeaker,
  onSetSelfSpeaker,
  onAssignSegmentSpeaker,
  onSpeakersDetected,
  transcriptViewportClassName,
}: TranscriptCardProps) {
  return (
    <div className="flex h-full min-h-0 flex-col">
      <TranscriptPanel
        transcripts={transcripts}
        customPrompt=""
        onPromptChange={() => {}}
        onCopyTranscript={() => {}}
        onOpenMeetingFolder={onOpenMeetingFolder}
        isRecording={false}
        disableAutoScroll
        transcriptViewportClassName={transcriptViewportClassName}
        surfaceClassName="bg-transparent"
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
        selfSpeakerIds={selfSpeakerIds}
        speakerCount={speakerCount}
        onRenameSpeaker={onRenameSpeaker}
        onSetSelfSpeaker={onSetSelfSpeaker}
        onAssignSegmentSpeaker={onAssignSegmentSpeaker}
        onSpeakersDetected={onSpeakersDetected}
        showToolbar={false}
        showContextField={false}
        showAudioPlayer={false}
      />
    </div>
  );
}
