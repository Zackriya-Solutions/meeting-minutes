'use client';

import { useRecordingState } from '@/contexts/RecordingStateContext';
import { useEffect, useState } from 'react';
import { formatElapsed } from './LiveIndicator';
import { cn } from '@/lib/utils';

/**
 * Sticky capture banner above the live transcript. Shows *active* duration
 * (pauses excluded), which is the number that matches the transcript below —
 * the rail's LiveIndicator shows wall-clock duration instead.
 */
export const RecordingStatusBar: React.FC<{ isPaused?: boolean }> = ({
  isPaused = false,
}) => {
  const { activeDuration } = useRecordingState();
  const [displaySeconds, setDisplaySeconds] = useState(0);

  useEffect(() => {
    if (activeDuration !== null) setDisplaySeconds(Math.floor(activeDuration));
  }, [activeDuration]);

  return (
    <div
      role="status"
      aria-live="polite"
      className={cn(
        'mb-2 flex items-center gap-2 rounded-md border px-2.5 py-1.5',
        isPaused
          ? 'border-warn/30 bg-warn-soft'
          : 'border-danger/25 bg-danger-soft'
      )}
    >
      <span
        aria-hidden
        className={cn(
          'h-1.5 w-1.5 shrink-0 rounded-full',
          isPaused ? 'bg-warn' : 'bg-danger animate-live'
        )}
      />
      <span
        className={cn(
          'text-sm font-medium',
          isPaused ? 'text-warn-ink' : 'text-danger-ink'
        )}
      >
        {isPaused ? 'Paused' : 'Recording'}
      </span>
      <span className="readout ml-auto text-2xs text-ink-muted">
        {formatElapsed(displaySeconds)}
      </span>
    </div>
  );
};
