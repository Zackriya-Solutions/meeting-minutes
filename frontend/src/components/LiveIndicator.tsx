'use client';

import { useRecordingState } from '@/contexts/RecordingStateContext';
import { cn } from '@/lib/utils';

export function formatElapsed(seconds: number | null | undefined) {
  const total = Math.max(0, Math.floor(seconds ?? 0));
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  const pad = (n: number) => String(n).padStart(2, '0');
  return h > 0 ? `${h}:${pad(m)}:${pad(s)}` : `${m}:${pad(s)}`;
}

/**
 * Capture state, readable three ways at once: a shape (filled dot), a word
 * ("Recording" / "Paused"), and an elapsed timer. Never color alone — a user
 * who cannot distinguish the red still gets the state from the label, and a
 * user with reduced motion still gets it from the timer advancing.
 */
export function LiveIndicator({
  compact = false,
  className,
}: {
  compact?: boolean;
  className?: string;
}) {
  const { isRecording, isPaused, recordingDuration } = useRecordingState();

  if (!isRecording) return null;

  return (
    <span
      role="status"
      aria-live="polite"
      className={cn('flex items-center gap-2', className)}
    >
      <span
        aria-hidden
        className={cn(
          'h-2 w-2 shrink-0 rounded-full',
          isPaused ? 'bg-warn' : 'bg-danger animate-live'
        )}
      />
      {!compact && (
        <span
          className={cn(
            'text-sm font-medium',
            isPaused ? 'text-warn-ink' : 'text-danger-ink'
          )}
        >
          {isPaused ? 'Paused' : 'Recording'}
        </span>
      )}
      <span className="readout text-2xs text-ink-muted tabular-nums">
        {formatElapsed(recordingDuration)}
      </span>
    </span>
  );
}
