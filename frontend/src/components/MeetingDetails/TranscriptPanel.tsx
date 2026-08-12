"use client";

import { Transcript, TranscriptSegmentData } from '@/types';
import { TranscriptView } from '@/components/TranscriptView';
import { VirtualizedTranscriptView } from '@/components/VirtualizedTranscriptView';
import { TranscriptButtonGroup } from './TranscriptButtonGroup';
import { Icon } from '@/components/memento/Icon';
import { useT } from '@/lib/i18n';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { convertFileSrc, invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import { useAudioPlayer } from '@/hooks/useAudioPlayer';
import { MeetingAudioPlayer } from './MeetingAudioPlayer';
import { cn } from '@/lib/utils';

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
  transcriptViewportClassName?: string;
  /** Optional surface override for embedded variants such as the meeting drawer. */
  surfaceClassName?: string;

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

  /** A bounded audio excerpt requested by the speaker identity review panel. */
  playbackRequest?: {
    requestId: number;
    meetingId: string;
    startSeconds: number;
    endSeconds: number;
  } | null;

  /** Marked moments (elapsed seconds) captured during recording. */
  markedMoments?: number[];
  /** Jump the transcript to a marked moment. */
  onSeekToMoment?: (seconds: number) => void;

  // Speaker diarization props
  speakersById?: Map<number, string> | null;
  selfSpeakerIds?: ReadonlySet<number> | null;
  speakerCount?: number;
  onRenameSpeaker?: (speakerId: number, displayName: string) => Promise<void> | void;
  onSetSelfSpeaker?: (speakerId: number, isSelf: boolean) => Promise<void> | void;
  /** Refresh speakers + transcripts after a successful detect. */
  onSpeakersDetected?: () => Promise<void> | void;

  /**
   * Presentation flags for the meeting-conversation transcript pin (variant 2a).
   * The pin moves the transcript actions into the screen's "⋯" menu and drops the
   * summary context field, so it renders the panel without its own toolbar/field
   * and with a compact audio player. All default to the original two-panel look.
   */
  showToolbar?: boolean;
  showContextField?: boolean;
  showAudioPlayer?: boolean;
  compactPlayer?: boolean;
}

export function TranscriptPanel({
  transcripts,
  customPrompt,
  onPromptChange,
  onCopyTranscript,
  onOpenMeetingFolder,
  isRecording,
  disableAutoScroll = false,
  transcriptViewportClassName,
  surfaceClassName,
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
  playbackRequest = null,
  markedMoments = [],
  onSeekToMoment,
  speakersById = null,
  selfSpeakerIds = null,
  speakerCount = 0,
  onRenameSpeaker,
  onSetSelfSpeaker,
  onSpeakersDetected,
  showToolbar = true,
  showContextField = true,
  showAudioPlayer = true,
  compactPlayer = false,
}: TranscriptPanelProps) {
  const t = useT();
  const [audioSources, setAudioSources] = useState<string[]>([]);
  const [savedAudioDuration, setSavedAudioDuration] = useState(0);
  const [audioUnavailable, setAudioUnavailable] = useState(false);
  const [isExportingMp3, setIsExportingMp3] = useState(false);
  const [activeExcerptEnd, setActiveExcerptEnd] = useState<number | null>(null);
  const handledPlaybackRequest = useRef<number | null>(null);
  const audio = useAudioPlayer(audioSources);

  useEffect(() => {
    let cancelled = false;
    setAudioSources([]);
    setSavedAudioDuration(0);
    setAudioUnavailable(false);
    setActiveExcerptEnd(null);
    if (!meetingId || !meetingFolderPath) return;

    invoke<{ path: string; playback_url?: string; duration_seconds: number }>('get_meeting_audio_playback_info', { meetingId })
      .then((info) => {
        if (!cancelled) {
          // WKWebView support differs between macOS versions. Prefer the local
          // byte-range HTTP transport (a real HTTP resource the media engine
          // streams and seeks reliably) and fall back to Tauri's scoped asset
          // protocol. The asset protocol can stall silently on some macOS builds
          // — emitting neither `playing` nor `error` — which previously left the
          // play button spinning forever.
          setAudioSources([info.playback_url, convertFileSrc(info.path)].filter(Boolean) as string[]);
          setSavedAudioDuration(info.duration_seconds);
        }
      })
      .catch((error) => {
        console.warn('Saved meeting has no playable audio:', error);
        if (!cancelled) setAudioUnavailable(true);
      });
    return () => { cancelled = true; };
  }, [meetingId, meetingFolderPath]);

  useEffect(() => {
    if (!playbackRequest || handledPlaybackRequest.current === playbackRequest.requestId) return;
    if (playbackRequest.meetingId !== meetingId) return;
    if (audioUnavailable) {
      handledPlaybackRequest.current = playbackRequest.requestId;
      toast.error(t('Audio is not available for this meeting'));
      return;
    }
    // Playback metadata is loaded asynchronously. Leave the request pending until at least
    // one source is ready instead of silently dropping the user's first click.
    if (audioSources.length === 0) return;
    handledPlaybackRequest.current = playbackRequest.requestId;
    setActiveExcerptEnd(playbackRequest.endSeconds);
    void audio.playFrom(playbackRequest.startSeconds);
  }, [audio.playFrom, audioSources.length, audioUnavailable, meetingId, playbackRequest, t]);

  useEffect(() => {
    if (activeExcerptEnd == null || !audio.isPlaying || audio.currentTime < activeExcerptEnd) return;
    audio.pause();
    setActiveExcerptEnd(null);
  }, [activeExcerptEnd, audio.currentTime, audio.isPlaying, audio.pause]);

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
    if (audioSources.length === 0) return;
    setActiveExcerptEnd(null);
    void audio.playFrom(seconds);
  }, [audio, audioSources.length]);

  const handleMarkedMoment = useCallback((seconds: number) => {
    onSeekToMoment?.(seconds);
    handlePlayTimestamp(seconds);
  }, [handlePlayTimestamp, onSeekToMoment]);
  const handleCorrectTranscript = useCallback(async (transcriptId: string, correctedText: string) => {
    try {
      const result = await invoke<{ terminology_candidate_id?: number | null }>('correct_transcript_segment', {
        input: { transcriptId, correctedText },
      });
      await onRefetchTranscripts?.();
      toast.success(
        result.terminology_candidate_id
          ? t('Correction saved and terminology suggested')
          : t('Transcript correction saved'),
      );
    } catch (error) {
      console.error('Failed to correct transcript:', error);
      toast.error(`${t('Failed to save transcript correction')}: ${String(error)}`);
      throw error;
    }
  }, [onRefetchTranscripts, t]);
  // Convert transcripts to segments if pagination is not used but we want virtualization
  const convertedSegments = useMemo(() => {
    if (usePagination && segments) {
      return segments;
    }
    // Convert transcripts to segments for virtualization
    return transcripts.map(t => ({
      id: t.id,
      timestamp: t.audio_start_time ?? 0,
      recognizedAt: t.timestamp,
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
    <div className={cn("relative flex h-full w-full min-w-0 flex-col bg-background", surfaceClassName)}>
      {/* Title area */}
      {showToolbar && (
        <div className="border-b border-border p-4">
          <TranscriptButtonGroup
            transcriptCount={usePagination ? (totalCount ?? convertedSegments.length) : (transcripts?.length || 0)}
            onCopyTranscript={onCopyTranscript}
            onToggleRecordingAudio={() => {
              if (audio.isPlaying) audio.pause();
              else void audio.play();
            }}
            recordingAudioAvailable={audioSources.length > 0 && !audioUnavailable}
            isRecordingAudioPlaying={audio.isPlaying}
            isRecordingAudioLoading={audio.isLoading}
            meetingId={meetingId}
            meetingFolderPath={meetingFolderPath}
            onRefetchTranscripts={onRefetchTranscripts}
            onSpeakersDetected={onSpeakersDetected}
            speakerCount={speakerCount}
          />
        </div>
      )}

      {showAudioPlayer && meetingId && meetingFolderPath && (
        <MeetingAudioPlayer
          compact={compactPlayer}
          available={audioSources.length > 0 && !audioUnavailable}
          isPlaying={audio.isPlaying}
          isLoading={audio.isLoading}
          isExporting={isExportingMp3}
          currentTime={audio.currentTime}
          duration={audio.duration > 0 ? audio.duration : savedAudioDuration}
          error={audio.error}
          onPlay={() => { setActiveExcerptEnd(null); void audio.play(); }}
          onPause={audio.pause}
          onSeek={(seconds) => { setActiveExcerptEnd(null); void audio.seek(seconds); }}
          onExportMp3={() => { void handleExportMp3(); }}
          onOpenFolder={() => { void onOpenMeetingFolder(); }}
        />
      )}

      {/* Marked moments — click to jump the transcript to that point */}
      {markedMoments.length > 0 && (
        <div className="flex flex-wrap items-center gap-1.5 border-b border-border px-4 py-2">
          <span className="mm-eyebrow mr-1">{t('Moments')}</span>
          {markedMoments.map((seconds) => (
            <button
              key={seconds}
              type="button"
              onClick={() => handleMarkedMoment(seconds)}
              title={t('Open this moment in the transcript')}
              className="mm-numeric inline-flex items-center gap-1 rounded-full border border-primary/40 bg-primary/10 px-2 py-0.5 text-xs text-primary transition-colors hover:bg-primary/20"
            >
              <Icon name="dot" size={12} />
              {formatClock(seconds)}
            </button>
          ))}
        </div>
      )}

      {/* Transcript content - use virtualized view for better performance */}
      <div className="min-h-0 flex-1 overflow-hidden">
        <VirtualizedTranscriptView
          segments={convertedSegments}
          isRecording={isRecording}
          isPaused={false}
          isProcessing={false}
          isStopping={false}
          enableStreaming={false}
          showConfidence={true}
          disableAutoScroll={disableAutoScroll}
          viewportClassName={transcriptViewportClassName}
          hasMore={hasMore}
          isLoadingMore={isLoadingMore}
          totalCount={totalCount}
          loadedCount={loadedCount}
          onLoadMore={onLoadMore}
          scrollToTimestamp={scrollToTimestamp}
          speakersById={speakersById}
          selfSpeakerIds={selfSpeakerIds}
          onRenameSpeaker={onRenameSpeaker}
          onSetSelfSpeaker={onSetSelfSpeaker}
          onPlayTimestamp={handlePlayTimestamp}
          playbackTime={audioSources.length > 0 && (audio.isPlaying || audio.currentTime > 0) ? audio.currentTime : null}
          onCorrectTranscript={meetingId ? handleCorrectTranscript : undefined}
        />
      </div>

      {/* Custom prompt input at bottom of transcript section */}
      {showContextField && !isRecording && convertedSegments.length > 0 && (
        <div className="border-t border-border p-3">
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
