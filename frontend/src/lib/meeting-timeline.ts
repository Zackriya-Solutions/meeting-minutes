/**
 * Meeting timeline generation.
 *
 * Turns a list of transcript segments into a compact set of navigable
 * "key moments" (chapters, questions, action items and points where the
 * conversation resumes after a pause). The output powers the interactive
 * meeting timeline on the meeting-details page.
 *
 * This module is intentionally free of React and I/O so the heuristics can be
 * unit-tested in isolation and reused by any surface that has transcript data.
 */

/** The category a timeline marker belongs to. Drives its icon and styling. */
export type TimelineMarkerKind = 'chapter' | 'question' | 'action' | 'resume';

/**
 * A single transcript segment, expressed in recording-relative seconds.
 *
 * This mirrors the fields the meeting-details page already holds
 * (`TranscriptSegmentData`) but is kept local so the generator does not depend
 * on UI types.
 */
export interface TimelineSegment {
  id: string;
  /** Seconds from the start of the recording. */
  startTime: number;
  /** Seconds from the start of the recording. Optional for legacy data. */
  endTime?: number;
  /** The transcribed text for this segment. */
  text: string;
}

/** A key moment surfaced on the timeline. */
export interface TimelineMarker {
  /** Stable identifier, unique within a generated timeline. */
  id: string;
  /** Position in seconds from the start of the recording. */
  time: number;
  /** Short, human-readable description of the moment. */
  label: string;
  /** The category of moment, used for icon and colour. */
  kind: TimelineMarkerKind;
  /** Id of the transcript segment this marker points to. */
  segmentId: string;
  /** Index of that segment within the input array (for fast scroll-to). */
  segmentIndex: number;
}

/** Tunable knobs for {@link generateTimeline}. All fields are optional. */
export interface TimelineOptions {
  /** Target number of evenly spaced chapters across the meeting. */
  targetChapters?: number;
  /** Silence gap (seconds) between segments that flags a "resume" marker. */
  pauseThresholdSeconds?: number;
  /** Minimum spacing (seconds) enforced between two adjacent markers. */
  minMarkerSpacingSeconds?: number;
  /** Hard cap on the number of markers returned. */
  maxMarkers?: number;
}

/** Default generation options, tuned for typical meeting lengths. */
export const DEFAULT_TIMELINE_OPTIONS: Required<TimelineOptions> = {
  targetChapters: 6,
  pauseThresholdSeconds: 20,
  minMarkerSpacingSeconds: 8,
  maxMarkers: 24,
};

/**
 * Relative importance of each marker kind. When two markers fall too close
 * together, the higher-priority one wins. Content-bearing moments (action
 * items, questions) are considered more informative than generic chapter
 * boundaries.
 */
const KIND_PRIORITY: Record<TimelineMarkerKind, number> = {
  action: 4,
  question: 3,
  resume: 2,
  chapter: 1,
};

/**
 * Phrases that typically signal a commitment or task. Matched
 * case-insensitively against whole words to limit false positives.
 */
const ACTION_PATTERNS: readonly RegExp[] = [
  /\baction item\b/i,
  /\bto-?dos?\b/i,
  /\bfollow[-\s]?ups?\b/i,
  /\b(?:i|we|you|they)['’]?ll\b/i,
  /\b(?:i|we|you|they)\s+will\b/i,
  /\bwe (?:need|have) to\b/i,
  /\bwe should\b/i,
  /\blet[’']?s\b/i,
  /\bassigned?\b/i,
  /\btake care of\b/i,
  /\bmake sure\b/i,
  /\bnext steps?\b/i,
  /\bby (?:tomorrow|today|eod|end of day|monday|tuesday|wednesday|thursday|friday|next week)\b/i,
];

/**
 * Formats a duration in seconds as a timecode.
 *
 * Uses `M:SS` for meetings under an hour and `H:MM:SS` beyond that. Negative
 * or non-finite input is clamped to `0:00`.
 */
export function formatTimecode(seconds: number): string {
  const safe = Number.isFinite(seconds) && seconds > 0 ? Math.floor(seconds) : 0;
  const hours = Math.floor(safe / 3600);
  const minutes = Math.floor((safe % 3600) / 60);
  const secs = safe % 60;

  const paddedSecs = secs.toString().padStart(2, '0');
  if (hours > 0) {
    const paddedMinutes = minutes.toString().padStart(2, '0');
    return `${hours}:${paddedMinutes}:${paddedSecs}`;
  }
  return `${minutes}:${paddedSecs}`;
}

/**
 * Produces a short, single-line label from a segment's text.
 *
 * Collapses whitespace, cuts at the first sentence boundary when one appears
 * early, and otherwise truncates on a word boundary with an ellipsis.
 */
export function summarizeSegmentText(text: string, maxLength = 60): string {
  const normalized = text.replace(/\s+/g, ' ').trim();
  if (normalized.length === 0) {
    return 'Discussion';
  }

  // Prefer cutting at the first sentence boundary if it yields a usable label.
  const sentenceMatch = normalized.match(/^(.+?[.!?])(\s|$)/);
  if (sentenceMatch) {
    const sentence = sentenceMatch[1].trim();
    if (sentence.length <= maxLength) {
      return stripTrailingPunctuation(sentence);
    }
  }

  if (normalized.length <= maxLength) {
    return stripTrailingPunctuation(normalized);
  }

  const truncated = normalized.slice(0, maxLength);
  const lastSpace = truncated.lastIndexOf(' ');
  const clipped = lastSpace > maxLength * 0.5 ? truncated.slice(0, lastSpace) : truncated;
  return `${clipped.trim()}…`;
}

function stripTrailingPunctuation(text: string): string {
  return text.replace(/[.,;:!?]+$/, '').trim();
}

/**
 * Classifies a segment as an action item or a question, or neither.
 * Action items take precedence over questions when both signals are present.
 */
export function classifySegment(text: string): 'action' | 'question' | null {
  const normalized = text.trim();
  if (normalized.length === 0) {
    return null;
  }
  if (ACTION_PATTERNS.some((pattern) => pattern.test(normalized))) {
    return 'action';
  }
  if (normalized.includes('?')) {
    return 'question';
  }
  return null;
}

/**
 * Generates a timeline of key moments from transcript segments.
 *
 * The algorithm is deterministic:
 * 1. Invalid segments (non-finite or negative start time, empty text) are
 *    dropped and the rest are sorted by start time.
 * 2. Evenly spaced chapter markers form a navigation backbone.
 * 3. Per-segment heuristics add question and action-item markers.
 * 4. Long silence gaps add "resume" markers.
 * 5. Markers closer than `minMarkerSpacingSeconds` are merged, keeping the
 *    higher-priority marker.
 * 6. The result is capped at `maxMarkers`, keeping the highest-priority markers
 *    and returned in chronological order.
 *
 * @returns Chronologically ordered markers. Empty when there are no valid
 *          segments.
 */
export function generateTimeline(
  segments: readonly TimelineSegment[],
  options: TimelineOptions = {},
): TimelineMarker[] {
  const config = { ...DEFAULT_TIMELINE_OPTIONS, ...options };

  const valid = segments
    .map((segment, index) => ({ segment, index }))
    .filter(
      ({ segment }) =>
        Number.isFinite(segment.startTime) &&
        segment.startTime >= 0 &&
        segment.text.trim().length > 0,
    )
    .sort((a, b) => a.segment.startTime - b.segment.startTime);

  if (valid.length === 0) {
    return [];
  }

  const candidates: TimelineMarker[] = [
    ...buildChapterMarkers(valid, config.targetChapters),
    ...buildEventMarkers(valid),
    ...buildResumeMarkers(valid, config.pauseThresholdSeconds),
  ];

  const merged = mergeBySpacing(candidates, config.minMarkerSpacingSeconds);
  const capped = capByPriority(merged, config.maxMarkers);

  return capped.sort((a, b) => a.time - b.time);
}

type IndexedSegment = { segment: TimelineSegment; index: number };

/**
 * Builds the chapter backbone by bucketing the meeting duration into
 * `targetChapters` evenly spaced windows and picking the first segment in each.
 */
function buildChapterMarkers(valid: IndexedSegment[], targetChapters: number): TimelineMarker[] {
  const chapters = Math.max(1, Math.floor(targetChapters));
  const first = valid[0];
  const last = valid[valid.length - 1];
  const span = last.segment.startTime - first.segment.startTime;

  // A single meaningful chapter for very short meetings or a single segment.
  if (span <= 0 || valid.length === 1) {
    return [toMarker(first, 'chapter', 'chapter')];
  }

  const bucketSize = span / chapters;
  const markers: TimelineMarker[] = [];
  let searchFrom = 0;

  for (let bucket = 0; bucket < chapters; bucket++) {
    const bucketStart = first.segment.startTime + bucket * bucketSize;
    const pick = findFirstAtOrAfter(valid, bucketStart, searchFrom);
    if (pick === null) {
      break;
    }
    searchFrom = pick.position + 1;
    markers.push(toMarker(pick.item, 'chapter', `chapter-${bucket}`));
  }

  return markers;
}

/** Adds a question/action marker for every segment that matches a heuristic. */
function buildEventMarkers(valid: IndexedSegment[]): TimelineMarker[] {
  const markers: TimelineMarker[] = [];
  for (const item of valid) {
    const kind = classifySegment(item.segment.text);
    if (kind !== null) {
      markers.push(toMarker(item, kind, `event-${item.segment.id}`));
    }
  }
  return markers;
}

/**
 * Adds a "resume" marker for the segment that follows a silence gap larger than
 * `pauseThresholdSeconds`.
 */
function buildResumeMarkers(valid: IndexedSegment[], pauseThresholdSeconds: number): TimelineMarker[] {
  if (pauseThresholdSeconds <= 0) {
    return [];
  }

  const markers: TimelineMarker[] = [];
  for (let i = 1; i < valid.length; i++) {
    const previous = valid[i - 1].segment;
    const current = valid[i].segment;
    const previousEnd = Number.isFinite(previous.endTime as number)
      ? (previous.endTime as number)
      : previous.startTime;
    const gap = current.startTime - previousEnd;
    if (gap >= pauseThresholdSeconds) {
      markers.push(toMarker(valid[i], 'resume', `resume-${current.id}`));
    }
  }
  return markers;
}

/**
 * Greedy chronological merge that enforces a minimum spacing between markers.
 * When a marker falls within `minSpacing` of the previously kept one, the
 * higher-priority marker is retained.
 */
function mergeBySpacing(markers: TimelineMarker[], minSpacing: number): TimelineMarker[] {
  const sorted = [...markers].sort(
    (a, b) => a.time - b.time || KIND_PRIORITY[b.kind] - KIND_PRIORITY[a.kind],
  );

  const kept: TimelineMarker[] = [];
  for (const marker of sorted) {
    const last = kept[kept.length - 1];
    if (!last || marker.time - last.time >= minSpacing) {
      kept.push(marker);
      continue;
    }
    if (KIND_PRIORITY[marker.kind] > KIND_PRIORITY[last.kind]) {
      kept[kept.length - 1] = marker;
    }
  }
  return kept;
}

/**
 * Keeps at most `maxMarkers`, preferring higher-priority markers and breaking
 * ties by earlier time. Order is not guaranteed; callers re-sort by time.
 */
function capByPriority(markers: TimelineMarker[], maxMarkers: number): TimelineMarker[] {
  const cap = Math.max(1, Math.floor(maxMarkers));
  if (markers.length <= cap) {
    return markers;
  }
  return [...markers]
    .sort((a, b) => KIND_PRIORITY[b.kind] - KIND_PRIORITY[a.kind] || a.time - b.time)
    .slice(0, cap);
}

/** Finds the first segment at or after `time`, searching from `fromPosition`. */
function findFirstAtOrAfter(
  valid: IndexedSegment[],
  time: number,
  fromPosition: number,
): { item: IndexedSegment; position: number } | null {
  for (let position = fromPosition; position < valid.length; position++) {
    if (valid[position].segment.startTime >= time) {
      return { item: valid[position], position };
    }
  }
  return null;
}

/** Converts an indexed segment into a marker of the given kind. */
function toMarker(item: IndexedSegment, kind: TimelineMarkerKind, idSuffix: string): TimelineMarker {
  return {
    id: `marker-${idSuffix}`,
    time: item.segment.startTime,
    label: summarizeSegmentText(item.segment.text),
    kind,
    segmentId: item.segment.id,
    segmentIndex: item.index,
  };
}

/**
 * Returns the id of the marker that is "active" at the given playback time —
 * the last marker whose time is at or before `timeSeconds`. Assumes `markers`
 * are in chronological order (as returned by {@link generateTimeline}).
 *
 * @returns The active marker id, or null when there is no marker at/before the
 *          time (or the time is negative/non-finite).
 */
export function findActiveMarkerId(
  markers: readonly TimelineMarker[],
  timeSeconds: number,
): string | null {
  if (!Number.isFinite(timeSeconds) || timeSeconds < 0) {
    return null;
  }
  let activeId: string | null = null;
  for (const marker of markers) {
    if (marker.time <= timeSeconds) {
      activeId = marker.id;
    } else {
      break;
    }
  }
  return activeId;
}

/** Minimal segment shape needed to resolve the active segment at a time. */
export interface PlayableSegment {
  id: string;
  /** Seconds from the start of the recording. */
  startTime: number;
}

/**
 * Returns the id of the transcript segment that is "active" at the given
 * playback time — the last segment that has started at or before `timeSeconds`.
 * Keeping the most recently started segment active means the highlight stays
 * stable through silence gaps rather than flickering off. Assumes `segments`
 * are in chronological order.
 *
 * @returns The active segment id, or null when playback is before the first
 *          segment (or the time is negative/non-finite).
 */
export function findActiveSegmentIdAtTime(
  segments: readonly PlayableSegment[],
  timeSeconds: number,
): string | null {
  if (!Number.isFinite(timeSeconds) || timeSeconds < 0) {
    return null;
  }
  let activeId: string | null = null;
  for (const segment of segments) {
    if (segment.startTime > timeSeconds) {
      break;
    }
    activeId = segment.id;
  }
  return activeId;
}
