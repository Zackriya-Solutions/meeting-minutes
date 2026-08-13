"use client";

import type { ReactNode } from 'react';
import { AlertTriangle, CheckCircle, CircleX, ExternalLink, Info } from '@/components/deslop-icons';
import { useT } from '@/lib/i18n';
import { cn } from '@/lib/utils';

/**
 * Shared pieces for the report sections rendered inside the meeting screen (score meters,
 * stat tiles, share bars, status chips, moment links).
 *
 * Two rules hold across all of them:
 *  - Severity lives in a mark (a bar fill, an icon), never in the text: the design
 *    system's accent hues sit well below 3:1 against the surface, so a green or orange
 *    label would be unreadable. Labels and values wear ink tokens.
 *  - A status is never colour alone — every chip carries an icon and a word, which is also
 *    what keeps the good/warn/bad trio apart for colour-blind readers.
 */

export type Tone = 'good' | 'warn' | 'bad' | 'neutral';

const TONE_COLOR: Record<Tone, string> = {
  good: 'var(--accent-green)',
  warn: 'var(--accent-orange)',
  bad: 'var(--accent-red)',
  neutral: 'var(--primary-40)',
};

/** Mirrors `meter_color` in the report renderer, so a meter reads the same in both. */
export function meterTone(percent: number): Tone {
  if (percent >= 65) return 'good';
  if (percent >= 40) return 'warn';
  return 'bad';
}

const TONE_ICON: Record<Tone, typeof CheckCircle> = {
  good: CheckCircle,
  warn: AlertTriangle,
  bad: CircleX,
  neutral: Info,
};

/** `mm:ss`, or `h:mm:ss` past the hour. Moment links and durations use the same clock. */
export function formatClock(seconds: number): string {
  const total = Math.max(0, Math.round(seconds));
  const hours = Math.floor(total / 3600);
  const minutes = Math.floor((total % 3600) / 60);
  const secs = total % 60;
  const mmss = `${String(minutes).padStart(2, '0')}:${String(secs).padStart(2, '0')}`;
  return hours > 0 ? `${hours}:${mmss}` : mmss;
}

/** Colour class for speaker series slot n (0..=3, then muted) — see globals.css. */
export function seriesClass(paletteIndex: number): string {
  return paletteIndex >= 0 && paletteIndex <= 3
    ? `mm-series-${paletteIndex + 1}`
    : 'mm-series-muted';
}

export function SectionHeading({
  children,
  note,
  action,
}: {
  children: ReactNode;
  note?: string;
  /** Trailing slot for a section-level action, e.g. "Открыть отчёт". */
  action?: ReactNode;
}) {
  return (
    <div className="flex flex-wrap items-baseline gap-x-2">
      <h2 className="text-sm font-normal leading-5 text-[var(--primary-50)]">{children}</h2>
      {note && <span className="min-w-0 flex-1 text-xs text-[var(--primary-40)]">{note}</span>}
      {action && <span className="ml-auto shrink-0">{action}</span>}
    </div>
  );
}

/** Opens the generated HTML report. Rendered only when a finished report exists. */
export function OpenReportButton({ onOpen }: { onOpen: () => void }) {
  const t = useT();
  return (
    <button
      type="button"
      onClick={onOpen}
      className="inline-flex items-center gap-1 rounded-md px-1 py-0.5 text-xs text-[var(--primary-50)] transition-colors hover:bg-[var(--primary-5)] hover:text-[var(--primary-70)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--primary-10)]"
    >
      <ExternalLink size={13} />
      {t('Open report')}
    </button>
  );
}

/** Jumps the meeting screen to the moment this claim came from. */
export function MomentLink({
  atSeconds,
  onSeek,
  label,
}: {
  atSeconds: number | null;
  onSeek?: (seconds: number) => void;
  label: string;
}) {
  if (atSeconds == null) return null;
  const clock = formatClock(atSeconds);
  if (!onSeek) {
    return <span className="text-xs text-[var(--primary-40)]">{clock}</span>;
  }
  return (
    <button
      type="button"
      onClick={() => onSeek(atSeconds)}
      title={label}
      aria-label={`${label} — ${clock}`}
      className="rounded-md px-1 py-0.5 text-xs text-[var(--primary-40)] transition-colors hover:bg-[var(--primary-5)] hover:text-[var(--primary-70)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--primary-10)]"
    >
      {clock}
    </button>
  );
}

/** One score component: label, a severity-filled bar, and its value. */
export function Meter({ label, value }: { label: string; value: number }) {
  const clamped = Math.max(0, Math.min(100, Math.round(value)));
  return (
    <div className="flex items-center gap-3">
      <span className="min-w-0 flex-1 truncate text-[length:var(--ui-body-font-size)] leading-[21px]">
        {label}
      </span>
      <div
        role="meter"
        aria-valuenow={clamped}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-label={label}
        className="h-1 w-[88px] shrink-0 overflow-hidden rounded-full bg-[var(--primary-10)]"
      >
        <div
          className="h-full rounded-full transition-[width] duration-700 ease-out motion-reduce:transition-none"
          style={{ width: `${clamped}%`, background: TONE_COLOR[meterTone(clamped)] }}
        />
      </div>
      <span className="mm-numeric w-[4ch] shrink-0 text-right text-sm text-[var(--primary-40)]">
        {clamped}
      </span>
    </div>
  );
}

export function StatusChip({
  tone,
  children,
  className,
}: {
  tone: Tone;
  children: ReactNode;
  className?: string;
}) {
  const Icon = TONE_ICON[tone];
  return (
    <span className={cn('inline-flex items-start gap-1 text-xs text-[var(--primary-60)]', className)}>
      <Icon size={14} color={TONE_COLOR[tone]} className="mt-[1px] shrink-0" />
      <span>{children}</span>
    </span>
  );
}

export function StatTile({
  label,
  value,
  sub,
}: {
  label: string;
  value: string;
  sub?: string | null;
}) {
  return (
    <div className="rounded-[14px] bg-[var(--primary-5)] px-3.5 py-3">
      <div className="text-xs leading-4 text-[var(--primary-50)]">{label}</div>
      <div className="mt-1.5 text-[22px] font-semibold leading-none text-foreground">{value}</div>
      {sub && <div className="mt-1.5 text-xs leading-4 text-[var(--primary-40)]">{sub}</div>}
    </div>
  );
}

/** A stage that failed, or found nothing, says so rather than leaving a gap. */
export function SectionPlaceholder({ children }: { children: ReactNode }) {
  return <p className="text-[length:var(--ui-body-font-size)] text-[var(--primary-40)]">{children}</p>;
}
