"use client";

import { Pause, Play } from 'lucide-react';
import { cn } from '@/lib/utils';
import { formatTimecode } from '@/lib/meeting-timeline';

export interface MeetingAudioPlayerProps {
  /** Whether audio is currently playing. */
  isPlaying: boolean;
  /** Current playback position in seconds. */
  currentTime: number;
  /** Total duration in seconds (0 until the audio has loaded). */
  duration: number;
  /** Error message to surface, if loading or playback failed. */
  error?: string | null;
  /** Start playback. */
  onPlay: () => void;
  /** Pause playback. */
  onPause: () => void;
  /** Seek to an absolute position in seconds. */
  onSeek: (seconds: number) => void;
  /** Optional additional class names for the outer container. */
  className?: string;
}

/**
 * Compact audio transport for a recorded meeting: a play/pause toggle, a
 * scrubber, and elapsed/total timecodes. Purely presentational — playback state
 * is owned by the parent (via the `useAudioPlayer` hook) so the timeline can
 * drive seeking too.
 */
export function MeetingAudioPlayer({
  isPlaying,
  currentTime,
  duration,
  error,
  onPlay,
  onPause,
  onSeek,
  className,
}: MeetingAudioPlayerProps) {
  const isReady = duration > 0;

  return (
    <div className={cn('flex items-center gap-3 px-3 py-2', className)}>
      <button
        type="button"
        onClick={isPlaying ? onPause : onPlay}
        disabled={!isReady}
        aria-label={isPlaying ? 'Pause recording' : 'Play recording'}
        className={cn(
          'flex h-8 w-8 flex-shrink-0 items-center justify-center rounded-full bg-blue-600 text-white transition-colors',
          'hover:bg-blue-700 focus:outline-none focus-visible:ring-2 focus-visible:ring-blue-500',
          'disabled:cursor-not-allowed disabled:bg-gray-300',
        )}
      >
        {isPlaying ? <Pause size={16} /> : <Play size={16} className="ml-0.5" />}
      </button>

      <span className="font-mono text-xs text-gray-500 tabular-nums">{formatTimecode(currentTime)}</span>

      <input
        type="range"
        min={0}
        max={isReady ? duration : 1}
        step={0.1}
        value={Math.min(currentTime, isReady ? duration : 1)}
        disabled={!isReady}
        onChange={(event) => onSeek(Number(event.target.value))}
        aria-label="Seek recording"
        className="h-1 flex-1 cursor-pointer accent-blue-600 disabled:cursor-not-allowed"
      />

      <span className="font-mono text-xs text-gray-400 tabular-nums">{formatTimecode(duration)}</span>

      {error && (
        <span className="text-xs text-red-500" role="alert">
          Audio unavailable
        </span>
      )}
    </div>
  );
}
