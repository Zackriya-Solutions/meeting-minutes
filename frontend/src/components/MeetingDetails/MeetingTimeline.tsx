"use client";

import { memo } from 'react';
import {
  BookOpen,
  ChevronDown,
  ChevronRight,
  HelpCircle,
  ListChecks,
  PlayCircle,
  type LucideIcon,
} from 'lucide-react';
import { cn } from '@/lib/utils';
import { formatTimecode, type TimelineMarker, type TimelineMarkerKind } from '@/lib/meeting-timeline';

/** Visual configuration for each marker kind: icon, accent colours and label. */
const KIND_CONFIG: Record<
  TimelineMarkerKind,
  { icon: LucideIcon; srLabel: string; iconClass: string; activeClass: string }
> = {
  chapter: {
    icon: BookOpen,
    srLabel: 'Chapter',
    iconClass: 'text-blue-500',
    activeClass: 'border-blue-500 bg-blue-50',
  },
  action: {
    icon: ListChecks,
    srLabel: 'Action item',
    iconClass: 'text-emerald-600',
    activeClass: 'border-emerald-500 bg-emerald-50',
  },
  question: {
    icon: HelpCircle,
    srLabel: 'Question',
    iconClass: 'text-violet-600',
    activeClass: 'border-violet-500 bg-violet-50',
  },
  resume: {
    icon: PlayCircle,
    srLabel: 'Resumes after pause',
    iconClass: 'text-amber-600',
    activeClass: 'border-amber-500 bg-amber-50',
  },
};

export interface MeetingTimelineProps {
  /** Key moments to render, in chronological order. */
  markers: TimelineMarker[];
  /** Id of the marker to highlight as active, if any. */
  activeMarkerId?: string | null;
  /** Invoked when the user selects a marker. */
  onMarkerSelect: (marker: TimelineMarker) => void;
  /** When true, the header toggles the list open and closed. */
  collapsible?: boolean;
  /** Whether the list is expanded. Only used when `collapsible` is true. */
  isOpen?: boolean;
  /** Invoked when the header is clicked. Only used when `collapsible` is true. */
  onToggleOpen?: () => void;
  /** Optional additional class names for the outer container. */
  className?: string;
}

/** A single, selectable timeline entry. Memoised to avoid needless re-renders. */
const TimelineEntry = memo(function TimelineEntry({
  marker,
  isActive,
  onSelect,
}: {
  marker: TimelineMarker;
  isActive: boolean;
  onSelect: (marker: TimelineMarker) => void;
}) {
  const config = KIND_CONFIG[marker.kind];
  const Icon = config.icon;

  return (
    <li>
      <button
        type="button"
        onClick={() => onSelect(marker)}
        aria-current={isActive ? 'true' : undefined}
        title={`${formatTimecode(marker.time)} — ${marker.label}`}
        className={cn(
          'group flex w-full items-start gap-2 rounded-md border border-transparent px-2 py-1.5 text-left transition-colors',
          'hover:bg-gray-100 focus:outline-none focus-visible:ring-2 focus-visible:ring-blue-500',
          isActive && config.activeClass,
        )}
      >
        <Icon
          size={16}
          className={cn('mt-0.5 flex-shrink-0', config.iconClass)}
          aria-hidden="true"
        />
        <span className="sr-only">{config.srLabel}:</span>
        <span className="flex min-w-0 flex-col">
          <span className="font-mono text-xs text-gray-400 tabular-nums">
            {formatTimecode(marker.time)}
          </span>
          <span className="truncate text-sm text-gray-700 group-hover:text-gray-900">
            {marker.label}
          </span>
        </span>
      </button>
    </li>
  );
});

/**
 * Renders an interactive list of meeting "key moments". Purely presentational:
 * the parent owns marker generation and selection state so this component stays
 * easy to test and reuse.
 */
export function MeetingTimeline({
  markers,
  activeMarkerId,
  onMarkerSelect,
  collapsible = false,
  isOpen = true,
  onToggleOpen,
  className,
}: MeetingTimelineProps) {
  const expanded = collapsible ? isOpen : true;
  const count = markers.length;
  const countLabel = count > 0 ? `${count} ${count === 1 ? 'moment' : 'moments'}` : null;

  const header = (
    <>
      <span className="flex items-center gap-1.5">
        {collapsible &&
          (expanded ? (
            <ChevronDown size={14} className="text-gray-400" aria-hidden="true" />
          ) : (
            <ChevronRight size={14} className="text-gray-400" aria-hidden="true" />
          ))}
        <span className="text-sm font-semibold text-gray-700">Timeline</span>
      </span>
      {countLabel && <span className="text-xs text-gray-400 tabular-nums">{countLabel}</span>}
    </>
  );

  return (
    <section aria-label="Meeting timeline" className={cn('flex min-h-0 flex-col bg-white', className)}>
      {collapsible ? (
        <button
          type="button"
          onClick={onToggleOpen}
          aria-expanded={expanded}
          className="flex items-center justify-between px-3 py-2 border-b border-gray-200 hover:bg-gray-50 focus:outline-none focus-visible:ring-2 focus-visible:ring-blue-500"
        >
          {header}
        </button>
      ) : (
        <div className="flex items-center justify-between px-3 py-2 border-b border-gray-200">{header}</div>
      )}

      {expanded &&
        (markers.length === 0 ? (
          <p className="px-3 py-4 text-xs text-gray-400">
            Key moments appear here once a meeting has been transcribed.
          </p>
        ) : (
          <ul className="flex flex-1 flex-col gap-0.5 overflow-y-auto p-2">
            {markers.map((marker) => (
              <TimelineEntry
                key={marker.id}
                marker={marker}
                isActive={marker.id === activeMarkerId}
                onSelect={onMarkerSelect}
              />
            ))}
          </ul>
        ))}
    </section>
  );
}
