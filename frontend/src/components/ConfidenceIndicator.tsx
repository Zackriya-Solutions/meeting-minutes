'use client';

import { cn } from '@/lib/utils';

/** Above this the decode is trustworthy and the UI says nothing about it. */
const QUIET_THRESHOLD = 0.8;

function describe(conf: number) {
  if (conf >= QUIET_THRESHOLD) return { label: 'High confidence', tone: 'brand' } as const;
  if (conf >= 0.7) return { label: 'Good confidence', tone: 'brand' } as const;
  if (conf >= 0.4) return { label: 'Uncertain', tone: 'warn' } as const;
  return { label: 'Low confidence', tone: 'danger' } as const;
}

/**
 * Decode confidence, shown only when it is worth acting on. A confident
 * transcript carries no marker at all — decorating every line with a green dot
 * is noise, and noise is what makes the real warnings invisible.
 *
 * `always` forces the readout (used in tooltips, where the user asked).
 */
export const ConfidenceIndicator: React.FC<{
  confidence: number;
  showIndicator?: boolean;
  always?: boolean;
}> = ({ confidence, showIndicator = true, always = false }) => {
  if (!showIndicator) return null;
  if (!always && confidence >= QUIET_THRESHOLD) return null;

  const { label, tone } = describe(confidence);
  const percent = Math.round(confidence * 100);

  return (
    <span
      className={cn(
        'inline-flex items-center gap-1 rounded-sm px-1 py-px align-middle',
        tone === 'danger' && 'bg-danger-soft text-danger-ink',
        tone === 'warn' && 'bg-warn-soft text-warn-ink',
        tone === 'brand' && 'bg-brand-soft text-brand-soft-ink'
      )}
      title={`${label} — ${percent}%`}
      aria-label={`Transcription confidence ${percent} percent, ${label}`}
    >
      <span className="readout text-2xs font-medium">{percent}%</span>
    </span>
  );
};
