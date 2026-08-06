import React from 'react';
import { cn } from '@/lib/utils';

/** Log-scaled 0..1 → 0..100, matching how loudness is actually perceived. */
function toPercent(level: number) {
  const n = Math.max(0, Math.min(1, level));
  return Math.round((n > 0 ? Math.log10(n * 9 + 1) : 0) * 100);
}

const SEGMENTS = 14;

/**
 * Segmented level meter — an instrument readout, not a progress bar. The
 * numeric percentage is always rendered alongside, so the level survives
 * reduced motion (which stops the segment transitions) and colour blindness
 * (green/amber/red segments are also positional).
 */
export function AudioLevelMeter({
  rmsLevel,
  peakLevel,
  isActive,
  deviceName,
  className = '',
  size = 'medium',
}: {
  rmsLevel: number;
  peakLevel: number;
  isActive: boolean;
  deviceName: string;
  className?: string;
  size?: 'small' | 'medium' | 'large';
}) {
  const rms = toPercent(rmsLevel);
  const peak = toPercent(peakLevel);
  const lit = Math.round((rms / 100) * SEGMENTS);
  const peakSegment = Math.round((peak / 100) * SEGMENTS);

  const barHeight = { small: 'h-2', medium: 'h-2.5', large: 'h-3.5' }[size];

  return (
    <div className={cn('flex items-center gap-2', className)}>
      <span
        aria-hidden
        title={`${deviceName} — ${isActive ? 'active' : 'silent'}`}
        className={cn(
          'h-1.5 w-1.5 shrink-0 rounded-full',
          isActive ? 'bg-brand' : 'bg-ink-faint/40'
        )}
      />

      <div
        role="meter"
        aria-label={`${deviceName} input level`}
        aria-valuenow={rms}
        aria-valuemin={0}
        aria-valuemax={100}
        className={cn('flex flex-1 items-stretch gap-px', barHeight)}
      >
        {Array.from({ length: SEGMENTS }, (_, i) => {
          // Top ~14% is clipping territory, next ~21% is hot.
          const tone =
            i >= SEGMENTS - 2 ? 'bg-danger' : i >= SEGMENTS - 5 ? 'bg-warn' : 'bg-brand';
          return (
            <span
              key={i}
              className={cn(
                'flex-1 rounded-[1px] transition-colors duration-fast',
                i < lit ? tone : i === peakSegment - 1 ? 'bg-ink-faint/50' : 'bg-ink/[0.09]'
              )}
            />
          );
        })}
      </div>

      <span className="readout min-w-[2.5rem] text-right text-2xs text-ink-muted">
        {rms}%
      </span>
    </div>
  );
}

/** Inline variant for device dropdowns — no numeric readout, no room for one. */
export function CompactAudioLevelMeter({
  rmsLevel,
  isActive,
  className = '',
}: {
  rmsLevel: number;
  peakLevel?: number;
  isActive: boolean;
  className?: string;
}) {
  const rms = toPercent(rmsLevel);

  return (
    <div className={cn('flex items-center gap-1.5', className)}>
      <span
        aria-hidden
        className={cn(
          'h-1.5 w-1.5 shrink-0 rounded-full',
          isActive ? 'bg-brand' : 'bg-ink-faint/40'
        )}
      />
      <div
        role="meter"
        aria-label="Input level"
        aria-valuenow={rms}
        aria-valuemin={0}
        aria-valuemax={100}
        className="h-1.5 w-10 overflow-hidden rounded-full bg-ink/[0.09]"
      >
        <div
          className={cn(
            'h-full rounded-full transition-[width] duration-fast',
            rms > 88 ? 'bg-danger' : rms > 68 ? 'bg-warn' : 'bg-brand'
          )}
          style={{ width: `${rms}%` }}
        />
      </div>
    </div>
  );
}
