"use client";

import { Transcript, TranscriptSegmentData } from '@/types';
import { TranscriptView } from '@/components/TranscriptView';
import { VirtualizedTranscriptView, type TranscriptViewHandle } from '@/components/VirtualizedTranscriptView';
import { TranscriptButtonGroup } from './TranscriptButtonGroup';
import { MeetingTimeline } from './MeetingTimeline';
import { MeetingAudioPlayer } from './MeetingAudioPlayer';
import {
  generateTimeline,
  findActiveMarkerId,
  findActiveSegmentIdAtTime,
  type TimelineMarker,
} from '@/lib/meeting-timeline';
import { useAudioPlayer } from '@/hooks/useAudioPlayer';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

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

  // Imperative handle to the transcript view for timeline-driven navigation.
  const transcriptViewRef = useRef<TranscriptViewHandle>(null);
  const [activeMarkerId, setActiveMarkerId] = useState<string | null>(null);
  const [activeSegmentId, setActiveSegmentId] = useState<string | null>(null);
  const [isTimelineOpen, setIsTimelineOpen] = useState(true);

  // Resolve the finalized meeting recording (saved as `audio.mp4` in the
  // meeting folder) so it can be played back and seeked from the timeline.
  const [audioPath, setAudioPath] = useState<string | null>(null);
  useEffect(() => {
    let cancelled = false;
    const resolveAudio = async () => {
      if (isRecording || !meetingFolderPath) {
        setAudioPath(null);
        return;
      }
      try {
        const { join } = await import('@tauri-apps/api/path');
        const { exists } = await import('@tauri-apps/plugin-fs');
        const candidate = await join(meetingFolderPath, 'audio.mp4');
        const found = await exists(candidate);
        if (!cancelled) setAudioPath(found ? candidate : null);
      } catch {
        if (!cancelled) setAudioPath(null);
      }
    };
    resolveAudio();
    return () => {
      cancelled = true;
    };
  }, [meetingFolderPath, isRecording]);

  const audio = useAudioPlayer(audioPath);

  // Derive timeline key moments from the loaded segments. Only meaningful once
  // recording has stopped and the transcript is available.
  const timelineMarkers = useMemo<TimelineMarker[]>(() => {
    if (isRecording) return [];
    return generateTimeline(
      convertedSegments.map((segment) => ({
        id: segment.id,
        startTime: segment.timestamp,
        endTime: segment.endTime,
        text: segment.text,
      })),
    );
  }, [convertedSegments, isRecording]);

  // Lightweight segment shape reused for resolving the active segment during
  // playback (see the follow-playback effect below).
  const playableSegments = useMemo(
    () => convertedSegments.map((segment) => ({ id: segment.id, startTime: segment.timestamp })),
    [convertedSegments],
  );

  const handleMarkerSelect = useCallback(
    (marker: TimelineMarker) => {
      setActiveMarkerId(marker.id);
      setActiveSegmentId(marker.segmentId);
      transcriptViewRef.current?.scrollToSegment(marker.segmentId);
      // Jump audio playback to the same moment when a recording is available.
      if (audioPath) {
        audio.seek(marker.time);
        audio.play();
      }
    },
    [audioPath, audio],
  );

  // While playing, keep the active marker and transcript segment in sync with
  // the playback position. Setting the same id is a no-op, so this stays cheap.
  useEffect(() => {
    if (!audio.isPlaying) return;
    const segmentId = findActiveSegmentIdAtTime(playableSegments, audio.currentTime);
    if (segmentId) setActiveSegmentId(segmentId);
    const markerId = findActiveMarkerId(timelineMarkers, audio.currentTime);
    if (markerId) setActiveMarkerId(markerId);
  }, [audio.currentTime, audio.isPlaying, playableSegments, timelineMarkers]);

  const handleToggleTimeline = useCallback(() => {
    setIsTimelineOpen((open) => !open);
  }, []);

  const showTimeline = !isRecording && timelineMarkers.length > 0;
  const showAudioPlayer = !isRecording && audioPath !== null;

  return (
    <div className="hidden md:flex md:w-1/4 lg:w-1/3 min-w-0 border-r border-gray-200 bg-white flex-col relative shrink-0">
      {/* Title area */}
      <div className="p-4 border-b border-gray-200">
        <TranscriptButtonGroup
          transcriptCount={usePagination ? (totalCount ?? convertedSegments.length) : (transcripts?.length || 0)}
          onCopyTranscript={onCopyTranscript}
          onOpenMeetingFolder={onOpenMeetingFolder}
          meetingId={meetingId}
          meetingFolderPath={meetingFolderPath}
          onRefetchTranscripts={onRefetchTranscripts}
        />
      </div>

      {/* Recorded audio transport (play / seek), when a recording exists */}
      {showAudioPlayer && (
        <div className="border-b border-gray-200">
          <MeetingAudioPlayer
            isPlaying={audio.isPlaying}
            currentTime={audio.currentTime}
            duration={audio.duration}
            error={audio.error}
            onPlay={audio.play}
            onPause={audio.pause}
            onSeek={audio.seek}
          />
        </div>
      )}

      {/* Interactive timeline of key moments (collapsible) */}
      {showTimeline && (
        <MeetingTimeline
          markers={timelineMarkers}
          activeMarkerId={activeMarkerId}
          onMarkerSelect={handleMarkerSelect}
          collapsible
          isOpen={isTimelineOpen}
          onToggleOpen={handleToggleTimeline}
          className="max-h-[38vh] border-b border-gray-200"
        />
      )}

      {/* Transcript content - use virtualized view for better performance */}
      <div className="flex-1 overflow-hidden pb-4">
        <VirtualizedTranscriptView
          ref={transcriptViewRef}
          segments={convertedSegments}
          isRecording={isRecording}
          isPaused={false}
          isProcessing={false}
          isStopping={false}
          enableStreaming={false}
          showConfidence={true}
          disableAutoScroll={disableAutoScroll}
          activeSegmentId={activeSegmentId}
          hasMore={hasMore}
          isLoadingMore={isLoadingMore}
          totalCount={totalCount}
          loadedCount={loadedCount}
          onLoadMore={onLoadMore}
        />
      </div>

      {/* Custom prompt input at bottom of transcript section */}
      {!isRecording && convertedSegments.length > 0 && (
        <div className="p-1 border-t border-gray-200">
          <textarea
            placeholder="Add context for AI summary. For example people involved, meeting overview, objective etc..."
            className="w-full px-3 py-2 border border-gray-200 rounded-md text-sm focus:outline-none focus:ring-1 focus:ring-blue-500 focus:border-blue-500 bg-white shadow-sm min-h-[80px] resize-y"
            value={customPrompt}
            onChange={(e) => onPromptChange(e.target.value)}
          />
        </div>
      )}
    </div>
  );
}
