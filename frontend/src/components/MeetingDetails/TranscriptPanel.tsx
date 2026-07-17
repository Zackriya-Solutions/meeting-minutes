"use client";

import { Transcript, TranscriptSegmentData } from '@/types';
import { TranscriptView } from '@/components/TranscriptView';
import { VirtualizedTranscriptView } from '@/components/VirtualizedTranscriptView';
import { TranscriptButtonGroup } from './TranscriptButtonGroup';
import { Icon } from '@/components/memento/Icon';
import { useT } from '@/lib/i18n';
import { useCallback, useEffect, useMemo, useState } from 'react';
import { convertFileSrc, invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import { useAudioPlayer } from '@/hooks/useAudioPlayer';
import { MeetingAudioPlayer } from './MeetingAudioPlayer';

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
  const t = useT();
  const [audioPath, setAudioPath] = useState<string | null>(null);
  const [savedAudioDuration, setSavedAudioDuration] = useState(0);
  const [audioUnavailable, setAudioUnavailable] = useState(false);
  const [isExportingMp3, setIsExportingMp3] = useState(false);
  const audio = useAudioPlayer(audioPath);

  useEffect(() => {
    let cancelled = false;
    setAudioPath(null);
    setSavedAudioDuration(0);
    setAudioUnavailable(false);
    if (!meetingId || !meetingFolderPath) return;

    invoke<{ path: string; duration_seconds: number }>('get_meeting_audio_playback_info', { meetingId })
      .then((info) => {
        if (!cancelled) {
          setAudioPath(convertFileSrc(info.path));
          setSavedAudioDuration(info.duration_seconds);
        }
      })
      .catch((error) => {
        console.warn('Saved meeting has no playable audio:', error);
        if (!cancelled) setAudioUnavailable(true);
      });
    return () => { cancelled = true; };
  }, [meetingId, meetingFolderPath]);

  const handleExportMp3 = useCallback(async () => {
    if (!meetingId || isExportingMp3) return;
    setIsExportingMp3(true);
    try {
      const exportedPath = await invoke<string | null>('export_meeting_audio_mp3', { meetingId });
      if (exportedPath) toast.success(t('MP3 export completed'));
    } catch (error) {
      console.error('Failed to export meeting audio as MP3:', error);
      toast.error(`${t('Failed to export MP3')}: ${String(error)}`);
    } finally {
      setIsExportingMp3(false);
    }
  }, [isExportingMp3, meetingId, t]);

  const handlePlayTimestamp = useCallback((seconds: number) => {
    if (!audioPath) return;
    void audio.playFrom(seconds);
  }, [audio, audioPath]);

  const handleMarkedMoment = useCallback((seconds: number) => {
    onSeekToMoment?.(seconds);
    handlePlayTimestamp(seconds);
  }, [handlePlayTimestamp, onSeekToMoment]);
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
    // Width and the panel divider are owned by the wrapper + splitter in
    // meeting-details/page-content.tsx; this root just fills its pane.
    <div className="relative flex h-full w-full min-w-0 flex-col bg-[var(--bg-sheet)]">
      {/* Title area */}
      <div className="border-b border-[var(--border-subtle)] p-4">
        <TranscriptButtonGroup
          transcriptCount={usePagination ? (totalCount ?? convertedSegments.length) : (transcripts?.length || 0)}
          onCopyTranscript={onCopyTranscript}
          onToggleRecordingAudio={() => {
            if (audio.isPlaying) audio.pause();
            else void audio.play();
          }}
          recordingAudioAvailable={!!audioPath && !audioUnavailable}
          isRecordingAudioPlaying={audio.isPlaying}
          isRecordingAudioLoading={audio.isLoading}
          meetingId={meetingId}
          meetingFolderPath={meetingFolderPath}
          onRefetchTranscripts={onRefetchTranscripts}
          onSpeakersDetected={onSpeakersDetected}
        />
      </div>

      {meetingId && meetingFolderPath && (
        <MeetingAudioPlayer
          available={!!audioPath && !audioUnavailable}
          isPlaying={audio.isPlaying}
          isLoading={audio.isLoading}
          isExporting={isExportingMp3}
          currentTime={audio.currentTime}
          duration={audio.duration > 0 ? audio.duration : savedAudioDuration}
          error={audio.error}
          onPlay={() => { void audio.play(); }}
          onPause={audio.pause}
          onSeek={(seconds) => { void audio.seek(seconds); }}
          onExportMp3={() => { void handleExportMp3(); }}
          onOpenFolder={() => { void onOpenMeetingFolder(); }}
        />
      )}

      {/* Marked moments — click to jump the transcript to that point */}
      {markedMoments.length > 0 && (
        <div className="flex flex-wrap items-center gap-1.5 border-b border-[var(--border-subtle)] px-4 py-2">
          <span className="mm-eyebrow mr-1">{t('Moments')}</span>
          {markedMoments.map((seconds) => (
            <button
              key={seconds}
              type="button"
              onClick={() => handleMarkedMoment(seconds)}
              title={t('Open this moment in the transcript')}
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
          onPlayTimestamp={handlePlayTimestamp}
          playbackTime={audioPath && (audio.isPlaying || audio.currentTime > 0) ? audio.currentTime : null}
        />
      </div>

      {/* Custom prompt input at bottom of transcript section */}
      {!isRecording && convertedSegments.length > 0 && (
        <div className="border-t border-[var(--border-subtle)] p-3">
          <textarea
            placeholder={t('Add context for AI summary. For example people involved, meeting overview, objective etc...')}
            className="mm-field min-h-[80px] w-full resize-y px-3 py-2 text-sm outline-none"
            value={customPrompt}
            onChange={(e) => onPromptChange(e.target.value)}
          />
        </div>
      )}
    </div>
  );
}
