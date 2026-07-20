"use client";

import { Button } from '@/components/ui/button';
import { Download, FolderOpen, Loader2, Pause, Play, Speaker } from '@/components/memento/LucideCompat';
import { useT } from '@/lib/i18n';

function formatTime(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds < 0) return '0:00';
  const whole = Math.floor(seconds);
  const hours = Math.floor(whole / 3600);
  const minutes = Math.floor((whole % 3600) / 60);
  const rest = whole % 60;
  return hours > 0
    ? `${hours}:${String(minutes).padStart(2, '0')}:${String(rest).padStart(2, '0')}`
    : `${minutes}:${String(rest).padStart(2, '0')}`;
}

interface MeetingAudioPlayerProps {
  available: boolean;
  isPlaying: boolean;
  isLoading: boolean;
  isExporting: boolean;
  currentTime: number;
  duration: number;
  error?: string | null;
  onPlay: () => void;
  onPause: () => void;
  onSeek: (seconds: number) => void;
  onExportMp3: () => void;
  onOpenFolder: () => void;
  /**
   * Compact variant for the transcript pin (variant 2a): hides the header row
   * (label + folder/export, which move to the "⋯" menu) and tightens spacing so
   * only the transport controls remain.
   */
  compact?: boolean;
}

export function MeetingAudioPlayer({
  available,
  isPlaying,
  isLoading,
  isExporting,
  currentTime,
  duration,
  error,
  onPlay,
  onPause,
  onSeek,
  onExportMp3,
  onOpenFolder,
  compact = false,
}: MeetingAudioPlayerProps) {
  const t = useT();

  return (
    <div
      className={
        compact
          ? 'rounded-[var(--radius-12)] border border-[var(--border-subtle)] bg-[var(--bg-canvas)] px-3 py-2'
          : 'border-b border-[var(--border-subtle)] bg-[var(--bg-canvas)] px-4 py-3'
      }
    >
      {!compact && (
      <div className="mb-2 flex items-center justify-between gap-2">
        <div className="flex min-w-0 items-center gap-2">
          <Speaker className="h-4 w-4 shrink-0 text-[var(--gold)]" />
          <div className="min-w-0">
            <p className="truncate text-xs font-medium text-[var(--fg1)]">{t('Meeting audio')}</p>
            <p className="truncate text-[10px] text-[var(--fg3)]">
              {available
                ? t('Click a transcript timestamp to play from that moment')
                : t('No saved audio is available for this meeting')}
            </p>
          </div>
        </div>
        <div className="flex shrink-0 items-center gap-1">
          <Button
            type="button"
            variant="outline"
            size="sm"
            className="h-7 w-7 p-0"
            onClick={onOpenFolder}
            title={t('Open Recording Folder')}
            aria-label={t('Open Recording Folder')}
          >
            <FolderOpen className="h-3.5 w-3.5" />
          </Button>
          <Button
            type="button"
            variant="outline"
            size="sm"
            className="h-7 shrink-0 px-2 text-[11px]"
            onClick={onExportMp3}
            disabled={!available || isExporting}
            title={t('Export a copy as MP3')}
          >
            {isExporting ? <Loader2 className="mr-1 h-3.5 w-3.5 animate-spin" /> : <Download className="mr-1 h-3.5 w-3.5" />}
            {t('Export MP3')}
          </Button>
        </div>
      </div>
      )}

      <div className="flex items-center gap-2">
        <button
          type="button"
          className="inline-flex h-8 w-8 shrink-0 items-center justify-center rounded-full bg-[var(--gold)] text-black transition-colors hover:bg-[var(--gold-active)] disabled:cursor-not-allowed disabled:opacity-40"
          onClick={isPlaying ? onPause : onPlay}
          disabled={!available || isLoading}
          aria-label={isPlaying ? t('Pause meeting audio') : t('Play meeting audio')}
          title={isPlaying ? t('Pause meeting audio') : t('Play meeting audio')}
        >
          {isLoading ? <Loader2 className="h-4 w-4 animate-spin" /> : isPlaying ? <Pause className="h-4 w-4" /> : <Play className="h-4 w-4" />}
        </button>
        <span className="mm-numeric w-10 shrink-0 text-right text-[11px] text-[var(--fg2)]">
          {formatTime(currentTime)}
        </span>
        <input
          type="range"
          min={0}
          max={duration > 0 ? duration : 1}
          step={0.1}
          value={duration > 0 ? Math.min(currentTime, duration) : 0}
          onChange={(event) => onSeek(Number(event.target.value))}
          disabled={!available || duration <= 0}
          aria-label={t('Audio position')}
          className="h-1 min-w-0 flex-1 cursor-pointer accent-[var(--gold)] disabled:cursor-not-allowed disabled:opacity-40"
        />
        <span className="mm-numeric w-10 shrink-0 text-[11px] text-[var(--fg2)]">
          {formatTime(duration)}
        </span>
      </div>

      {error && <p className="mt-1.5 text-[10px] text-[var(--danger)]">{t(error)}</p>}
    </div>
  );
}
