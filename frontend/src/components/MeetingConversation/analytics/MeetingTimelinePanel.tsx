"use client";

import { useMemo, useState, type ReactNode } from 'react';
import { useT } from '@/lib/i18n';
import type { AnalyticsTimelineSection } from '@/hooks/meeting-details/useMeetingAnalyticsSections';
import { SectionHeading, SectionPlaceholder, formatClock, seriesClass } from './primitives';

/**
 * The «Лента встречи» tab — the report's timeline: topic bands, event markers, and one
 * speech lane per speaker over a shared time axis. Clicking any block, band or marker jumps
 * the transcript tab to that second.
 *
 * Laid out in positioned HTML rather than the report's SVG on purpose: the drawer is as
 * narrow as 450px, and an SVG scaled from a fixed viewBox would shrink its type with it.
 * Percentages carry x, pixels carry y, so labels stay at real size at every width.
 *
 * Blocks are pointer-driven (hover for the reply, click to jump); with hundreds of replies
 * per meeting, making every one focusable would bury the keyboard path — the transcript tab
 * is that path, and topic bands and markers, which are few, are real buttons.
 */

// y geometry, in px. Mirrors the report's row order: topics, events, then speaker lanes.
const TOPICS_TOP = 0;
const TOPICS_H = 20;
const EVENTS_TOP = 26;
const EVENTS_H = 14;
const LANES_TOP = 46;
const LANE_GAP = 14;
const LANE_H = 10;
const TICKS_H = 14;
const LABEL_COLUMN = 58;

interface Tip {
  /** Percentage of the plot width, 0..100. */
  left: number;
  /** Bottom of the mark being described, in px — the tip hangs below it. */
  top: number;
  title: string;
  body?: string;
}

/**
 * Keep the tip inside the plot: it hangs below its mark and is centred on it, except near
 * the edges, where it anchors to the left or right instead of overflowing (the tab scrolls
 * vertically, so horizontal overflow would add a scrollbar).
 */
function tipShift(left: number): string {
  if (left < 20) return '0';
  if (left > 80) return '-100%';
  return '-50%';
}

export function MeetingTimelinePanel({
  timeline,
  onSeek,
  action,
}: {
  timeline: AnalyticsTimelineSection | null;
  onSeek?: (seconds: number) => void;
  action?: ReactNode;
}) {
  const t = useT();
  const [tip, setTip] = useState<Tip | null>(null);

  const duration = Math.max(timeline?.duration_secs ?? 0, 1);
  const lanes = timeline?.lanes ?? [];
  const turns = timeline?.turns ?? [];
  const topics = timeline?.topics ?? [];
  const markers = timeline?.markers ?? [];

  const lanesBottom = LANES_TOP + Math.max(0, lanes.length - 1) * LANE_GAP + LANE_H;
  const ticksTop = lanesBottom + 8;
  const plotHeight = ticksTop + TICKS_H;
  const pct = (seconds: number) => Math.max(0, Math.min(100, (seconds / duration) * 100));

  // Round tick spacing the same way the report does, so a long meeting stays readable.
  const ticks = useMemo(() => {
    const step = duration > 1800 ? 300 : duration > 600 ? 120 : 60;
    const out: number[] = [];
    for (let second = 0; second <= duration; second += step) out.push(second);
    return out;
  }, [duration]);

  const rowLabels = [
    { label: t('Topics row'), top: TOPICS_TOP + 5 },
    { label: t('Events row'), top: EVENTS_TOP + 2 },
    ...lanes.map((lane, index) => ({ label: lane.label, top: LANES_TOP + index * LANE_GAP - 2 })),
  ];

  return (
    <div className="mx-auto flex max-w-[720px] flex-col gap-4 pb-10 pt-1.5">
      <SectionHeading note={t('Topics, events and speech over time')} action={action}>
        {t('Meeting feed')}
      </SectionHeading>

      {turns.length === 0 ? (
        <SectionPlaceholder>{t('No speech data.')}</SectionPlaceholder>
      ) : (
        <>
          <div className="flex gap-2" onMouseLeave={() => setTip(null)}>
            {/* Row labels, aligned to the same y offsets as the plot rows. */}
            <div className="relative shrink-0" style={{ width: LABEL_COLUMN, height: plotHeight }}>
              {rowLabels.map((row) => (
                <div
                  key={`${row.label}-${row.top}`}
                  className="absolute right-0 truncate text-[10px] leading-[14px] text-[var(--primary-40)]"
                  style={{ top: row.top, maxWidth: LABEL_COLUMN }}
                  title={row.label}
                >
                  {row.label}
                </div>
              ))}
            </div>

            <div className="relative min-w-0 flex-1" style={{ height: plotHeight }}>
              {/* Time grid: hairlines behind everything, ticks under the lanes. */}
              {ticks.map((second) => (
                <div
                  key={`grid-${second}`}
                  aria-hidden="true"
                  className="absolute w-px bg-[var(--primary-10)]"
                  style={{ left: `${pct(second)}%`, top: 0, height: ticksTop }}
                />
              ))}
              {ticks.map((second, index) => {
                const position = pct(second);
                const shift = index === 0 ? '0' : position > 96 ? '-100%' : '-50%';
                return (
                  <div
                    key={`tick-${second}`}
                    className="mm-numeric absolute text-[10px] leading-[14px] text-[var(--primary-40)]"
                    style={{ left: `${position}%`, top: ticksTop, transform: `translateX(${shift})` }}
                  >
                    {formatClock(second)}
                  </div>
                );
              })}

              {/* Topic bands. A short topic in a long meeting is only a few pixels wide, so
                  its number is dropped rather than clipped — alternating tints still show
                  where one topic ends, and the key below (plus hover) names them all. */}
              {topics.map((topic, index) => {
                const width = Math.max(0, pct(topic.end) - pct(topic.start));
                return (
                  <button
                    key={`topic-${index}-${topic.start}`}
                    type="button"
                    onClick={() => onSeek?.(topic.start)}
                    onMouseEnter={() => setTip({
                      left: pct((topic.start + topic.end) / 2),
                      top: TOPICS_TOP + TOPICS_H + 4,
                      title: `${index + 1}. ${topic.name}`,
                      body: `${formatClock(topic.start)} – ${formatClock(topic.end)}`,
                    })}
                    aria-label={`${index + 1}. ${topic.name}, ${formatClock(topic.start)}`}
                    className={`absolute overflow-hidden rounded-[4px] border border-[var(--primary-10)] text-[10px] font-semibold text-[var(--primary-50)] hover:bg-[var(--primary-10)] ${
                      index % 2 === 0 ? 'bg-[var(--primary-5)]' : 'bg-[var(--primary-8)]'
                    }`}
                    style={{
                      left: `${pct(topic.start)}%`,
                      width: `${width}%`,
                      minWidth: 3,
                      top: TOPICS_TOP,
                      height: TOPICS_H,
                    }}
                  >
                    {width >= 4 ? index + 1 : ''}
                  </button>
                );
              })}

              {/* Events: shape carries the kind, the legend below names the shapes. */}
              {markers.map((marker, index) => (
                <button
                  key={`marker-${index}-${marker.at_seconds}`}
                  type="button"
                  onClick={() => onSeek?.(marker.at_seconds)}
                  onMouseEnter={() => setTip({
                    left: pct(marker.at_seconds),
                    top: EVENTS_TOP + EVENTS_H + 4,
                    title: `${t(MARKER_LABEL[marker.kind] ?? 'Event')}: ${marker.text}`,
                    body: formatClock(marker.at_seconds),
                  })}
                  aria-label={`${t(MARKER_LABEL[marker.kind] ?? 'Event')}: ${marker.text}, ${formatClock(marker.at_seconds)}`}
                  className="absolute flex items-center justify-center"
                  style={{
                    left: `${pct(marker.at_seconds)}%`,
                    top: EVENTS_TOP,
                    height: EVENTS_H,
                    width: 14,
                    transform: 'translateX(-50%)',
                  }}
                >
                  <MarkerShape kind={marker.kind} />
                </button>
              ))}

              {/* Speech blocks, one lane per speaker, coloured by the speaker series. */}
              {turns.map((turn, index) => (
                <div
                  key={`turn-${index}-${turn.start}`}
                  onClick={() => onSeek?.(turn.start)}
                  onMouseEnter={() => setTip({
                    left: pct(turn.start),
                    top: LANES_TOP + turn.lane * LANE_GAP + LANE_H + 4,
                    title: `${lanes[turn.lane]?.label ?? ''} · ${formatClock(turn.start)}`,
                    body: turn.text,
                  })}
                  className={`absolute cursor-pointer rounded-[2px] bg-current ${seriesClass(lanes[turn.lane]?.palette_index ?? 0)}`}
                  style={{
                    left: `${pct(turn.start)}%`,
                    width: `${Math.max(0, pct(turn.end) - pct(turn.start))}%`,
                    minWidth: 2,
                    top: LANES_TOP + turn.lane * LANE_GAP,
                    height: LANE_H,
                  }}
                />
              ))}

              {tip && (
                <div
                  className="pointer-events-none absolute z-10 w-[210px] rounded-[10px] border border-[var(--primary-10)] bg-[var(--elevation-2)] px-2.5 py-2 text-xs leading-4 text-foreground"
                  style={{
                    left: `${tip.left}%`,
                    top: tip.top,
                    transform: `translateX(${tipShift(tip.left)})`,
                  }}
                >
                  <div className="font-medium">{tip.title}</div>
                  {tip.body && <div className="mt-0.5 text-[var(--primary-50)]">{tip.body}</div>}
                </div>
              )}
            </div>
          </div>

          <p className="text-xs text-[var(--primary-40)]">
            {t('Feed legend')}
          </p>

          {topics.length > 0 && (
            <ol className="flex flex-col gap-1">
              {topics.map((topic, index) => (
                <li
                  key={`key-${index}-${topic.start}`}
                  className="flex gap-2 text-[length:var(--ui-body-font-size)] leading-[21px]"
                >
                  <span className="mm-numeric w-[2ch] shrink-0 text-right text-[var(--primary-40)]">
                    {index + 1}
                  </span>
                  <span className="min-w-0 flex-1 text-foreground">{topic.name}</span>
                  <span className="mm-numeric shrink-0 text-xs text-[var(--primary-40)]">
                    {formatClock(topic.start)} – {formatClock(topic.end)}
                  </span>
                </li>
              ))}
            </ol>
          )}
        </>
      )}
    </div>
  );
}

const MARKER_LABEL: Record<string, string> = {
  decision: 'Decision',
  disagreement: 'Disagreement',
  commitment: 'Commitment',
};

/** Circle = decision, triangle = disagreement, diamond = commitment (as in the report). */
function MarkerShape({ kind }: { kind: string }) {
  // A 1.5px surface ring keeps overlapping markers legible where they crowd.
  const ring = { boxShadow: '0 0 0 1.5px var(--elevation-1)' };
  if (kind === 'disagreement') {
    return (
      <span
        aria-hidden="true"
        className="block h-0 w-0"
        style={{
          borderLeft: '5px solid transparent',
          borderRight: '5px solid transparent',
          borderBottom: '9px solid var(--accent-orange)',
        }}
      />
    );
  }
  if (kind === 'commitment') {
    return (
      <span
        aria-hidden="true"
        className="block h-[8px] w-[8px] rotate-45"
        style={{ background: 'var(--primary-60)', ...ring }}
      />
    );
  }
  return (
    <span
      aria-hidden="true"
      className="block h-[9px] w-[9px] rounded-full"
      style={{ background: 'var(--primary-60)', ...ring }}
    />
  );
}
