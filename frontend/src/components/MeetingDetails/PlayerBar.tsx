'use client';

import { Pause, Play } from 'lucide-react';
import { usePlaybackClock, type PlaybackControls } from '@/hooks/usePlayback';

const RATES = [1, 1.5, 2] as const;

function formatClock(seconds: number): string {
  const total = Math.max(0, Math.floor(seconds));
  const hours = Math.floor(total / 3600);
  const minutes = String(Math.floor((total % 3600) / 60)).padStart(2, '0');
  const secs = String(total % 60).padStart(2, '0');
  return hours > 0 ? `${hours}:${minutes}:${secs}` : `${minutes}:${secs}`;
}

/**
 * Play/pause, seek bar, time and speed for the meeting's recording; nothing without audio.
 * The only component that follows the clock on every timeupdate.
 */
export function PlayerBar({ playback }: { playback: PlaybackControls }) {
  const clockS = usePlaybackClock(playback.clock);
  if (!playback.ready) return null;
  const { playing, durationS, rate, error } = playback;
  const nextRate = RATES[(RATES.indexOf(rate) + 1) % RATES.length];
  return (
    <div className="flex items-center gap-3 border-t border-gray-200 px-4 py-2 text-xs text-gray-600">
      <button
        type="button"
        aria-label={playing ? 'Pause' : 'Play'}
        title="Play or pause (Space)"
        className="inline-flex h-7 w-7 shrink-0 items-center justify-center rounded-full bg-blue-600 text-white hover:bg-blue-700"
        onClick={playback.toggle}
      >
        {playing ? <Pause className="h-3.5 w-3.5" /> : <Play className="h-3.5 w-3.5" />}
      </button>
      <span className="shrink-0 tabular-nums">{formatClock(clockS)}</span>
      <input
        type="range"
        aria-label="Seek"
        min={0}
        max={Math.max(durationS, 0.1)}
        step={0.1}
        value={Math.min(clockS, durationS)}
        onChange={(e) => playback.seek(Number(e.target.value))}
        className="min-w-0 flex-1 accent-blue-600"
      />
      <span className="shrink-0 tabular-nums">{formatClock(durationS)}</span>
      <button
        type="button"
        aria-label={`Playback speed ${rate}×, change to ${nextRate}×`}
        className="w-10 shrink-0 rounded border border-gray-300 py-0.5 hover:bg-gray-50"
        onClick={() => playback.setRate(nextRate)}
      >
        {`${rate}×`}
      </button>
      {error && <span className="min-w-0 truncate text-red-600" title={error}>{error}</span>}
    </div>
  );
}
