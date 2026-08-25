"use client";

import { Transcript, TranscriptSegmentData } from '@/types';
import { VirtualizedTranscriptView } from '@/components/VirtualizedTranscriptView';
import { TranscriptButtonGroup } from './TranscriptButtonGroup';
import { useMemo, useCallback } from 'react';
import { useTranslations } from 'next-intl';
import { useAudioPlayer } from '@/hooks/useAudioPlayer';
import { useSpeakerNames } from '@/hooks/useSpeakerNames';
import { Play, Pause, AlertCircle } from 'lucide-react';

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

  // Audio jump props (PR-44c): optional audio file path enables
  // transcript timestamp click-to-jump + compact player UI.
  audioPath?: string | null;
}

function formatTime(seconds: number): string {
  if (!isFinite(seconds) || seconds < 0) return '00:00';
  const m = Math.floor(seconds / 60);
  const s = Math.floor(seconds % 60);
  return `${m.toString().padStart(2, '0')}:${s.toString().padStart(2, '0')}`;
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
  audioPath,
}: TranscriptPanelProps) {
  const tSummary = useTranslations('summary');
  const tView = useTranslations('transcript.view');
  const audioPlayer = useAudioPlayer(audioPath ?? null);

  const convertedSegments = useMemo(() => {
    if (usePagination && segments) {
      return segments;
    }
    return transcripts.map(t => ({
      id: t.id,
      timestamp: t.audio_start_time ?? 0,
      endTime: t.audio_end_time,
      text: t.text,
      confidence: t.confidence,
      speaker: t.speaker,
    }));
  }, [transcripts, usePagination, segments]);

  const speakerNames = useSpeakerNames(meetingId ?? null);

  const handleTimestampClick = useCallback((sec: number) => {
    audioPlayer.seek(sec);
  }, [audioPlayer]);

  const hasAudio = !!audioPath;

  return (
    <div className="hidden md:flex md:w-1/4 lg:w-1/3 min-w-0 border-r border-gray-200 bg-white flex-col relative shrink-0">
      <div className="p-4 border-b border-gray-200">
        <TranscriptButtonGroup
          transcriptCount={usePagination ? (totalCount ?? convertedSegments.length) : (transcripts?.length || 0)}
          onCopyTranscript={onCopyTranscript}
          onOpenMeetingFolder={onOpenMeetingFolder}
          meetingId={meetingId}
          meetingFolderPath={meetingFolderPath}
          onRefetchTranscripts={onRefetchTranscripts}
        />

        {hasAudio && (
          <div className="mt-3 flex items-center gap-2 text-xs text-gray-600" data-testid="transcript-audio-player">
            <button
              type="button"
              onClick={() => audioPlayer.isPlaying ? audioPlayer.pause() : audioPlayer.play()}
              disabled={!!audioPlayer.error || audioPlayer.duration === 0}
              title={audioPlayer.isPlaying ? tView('pause_title') : tView('play_title')}
              aria-label={audioPlayer.isPlaying ? tView('pause_title') : tView('play_title')}
              className="flex items-center justify-center w-7 h-7 rounded-full border border-gray-300 hover:bg-gray-100 disabled:opacity-50 disabled:cursor-not-allowed text-gray-700"
            >
              {audioPlayer.isPlaying ? <Pause size={14} /> : <Play size={14} />}
            </button>
            <div className="flex-1 flex items-center gap-2 min-w-0">
              <span className="font-mono tabular-nums">{formatTime(audioPlayer.currentTime)}</span>
              <input
                type="range"
                min={0}
                max={audioPlayer.duration || 0}
                step={0.1}
                value={Math.min(audioPlayer.currentTime, audioPlayer.duration || 0)}
                onChange={(e) => audioPlayer.seek(parseFloat(e.target.value))}
                disabled={audioPlayer.duration === 0}
                title={tView('seek_title')}
                aria-label={tView('seek_title')}
                className="flex-1 h-1 accent-blue-600 disabled:opacity-50 min-w-0"
              />
              <span className="font-mono tabular-nums">{formatTime(audioPlayer.duration)}</span>
            </div>
            {audioPlayer.error && (
              <AlertCircle size={14} className="text-red-500 flex-shrink-0" aria-label={audioPlayer.error} />
            )}
          </div>
        )}
      </div>

      <div className="flex-1 overflow-hidden pb-4">
        <VirtualizedTranscriptView
          segments={convertedSegments}
          onTimestampClick={hasAudio ? handleTimestampClick : undefined}
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
          customSpeakerNames={speakerNames.allNames}
          onSpeakerRename={speakerNames.setName}
        />
      </div>

      {!isRecording && convertedSegments.length > 0 && (
        <div className="p-1 border-t border-gray-200">
          <textarea
            placeholder={tSummary("meeting.custom_prompt_placeholder")}
            className="w-full px-3 py-2 border border-gray-200 rounded-md text-sm focus:outline-none focus:ring-1 focus:ring-blue-500 focus:border-blue-500 bg-white shadow-sm min-h-[80px] resize-y"
            value={customPrompt}
            onChange={(e) => onPromptChange(e.target.value)}
          />
        </div>
      )}
    </div>
  );
}
