// VALUEOS: live-transcript state model. The native pipeline now streams INTERIM ("preview")
// hypotheses for the in-progress utterance (pipeline.rs periodically transcribes the not-yet-
// closed VAD buffer and emits is_partial:true), then a FINAL when the segment closes. The runtime
// live view is driven by `reduceLive` over the transcript-update event stream (see
// useRecordingController): each INTERIM replaces the single preview buffer (latest wins, never
// appended); the FINAL commits exactly once (deduped by sequence_id) and clears the preview.
// `deriveLive` is the equivalent snapshot form over a segment array (used in tests/fallbacks).
//
// INTERIM IS DISPLAY-ONLY. Only committed (final) segments flow into the enriched export/upload
// (see transcriptFormat.ts, which drops is_partial segments).

import { sourceToLabel } from './speakerLabels';

export interface LiveSegment {
  text: string;
  is_partial?: boolean;
  sequence_id?: number;
  source?: string | null;
  audio_start_time?: number;
}

export interface LiveState {
  /** Finalized segments (is_partial !== true), de-duped by sequence_id, in arrival order. */
  committed: LiveSegment[];
  /** The current in-progress hypothesis — the LATEST interim only (never a growing list). */
  interim: LiveSegment | null;
}

export const emptyLive: LiveState = { committed: [], interim: null };

function nonEmpty(s: LiveSegment): boolean {
  return (s.text ?? '').trim().length > 0;
}

/**
 * Apply one engine update. INTERIM (is_partial) → replace the single interim buffer (latest
 * wins). FINAL → commit it (dedup/replace by sequence_id) and CLEAR the interim.
 */
export function reduceLive(state: LiveState, update: LiveSegment): LiveState {
  if (update.is_partial) {
    return { committed: state.committed, interim: nonEmpty(update) ? update : null };
  }
  if (!nonEmpty(update)) {
    // An empty final just ends the current interim (nothing to commit).
    return { committed: state.committed, interim: null };
  }
  const key = update.sequence_id;
  const exists = key != null && state.committed.some((s) => s.sequence_id === key);
  const committed = exists
    ? state.committed.map((s) => (s.sequence_id === key ? update : s))
    : [...state.committed, update];
  return { committed, interim: null };
}

/** Snapshot derivation over the accumulated segment array (the current runtime path): a trailing
 *  in-progress segment is the interim; everything before it is committed. */
export function deriveLive(segments: LiveSegment[]): LiveState {
  if (segments.length === 0) return emptyLive;
  const last = segments[segments.length - 1];
  if (last?.is_partial) {
    return { committed: segments.slice(0, -1).filter(nonEmpty), interim: nonEmpty(last) ? last : null };
  }
  return { committed: segments.filter(nonEmpty), interim: null };
}

/** One rendered chat line in the live view. `me` = the local user (label "You", blue, left side);
 *  `other` = someone else, e.g. system/remote audio (label "Other", grey, right side). */
export interface LiveLine {
  role: 'me' | 'other';
  label: string;
  text: string;
  /** true for the in-progress (interim) hypothesis — rendered faded/italic. */
  partial?: boolean;
}

// Only an explicit "Other" speaker goes to the right; everything else (incl. no speaker signal)
// stays as the local user ("You"), so an un-attributed segment never renders as a stranger.
function roleOf(source?: string | null): 'me' | 'other' {
  return sourceToLabel(source) === 'Other' ? 'other' : 'me';
}

/**
 * Turn the live state into speaker-aligned chat lines. Consecutive same-speaker COMMITTED segments
 * merge into one bubble (clean chat look); the interim hypothesis, if any, is ALWAYS its own
 * trailing partial line (never merged, since it will be replaced). Empty text is dropped.
 */
export function toLiveLines(state: LiveState): LiveLine[] {
  const lines: LiveLine[] = [];
  for (const seg of state.committed) {
    const text = (seg.text ?? '').trim();
    if (!text) continue;
    const role = roleOf(seg.source);
    const last = lines[lines.length - 1];
    if (last && last.role === role && !last.partial) {
      last.text += ' ' + text;
    } else {
      lines.push({ role, label: role === 'me' ? 'You' : 'Other', text, partial: false });
    }
  }
  if (state.interim) {
    const text = (state.interim.text ?? '').trim();
    if (text) {
      const role = roleOf(state.interim.source);
      lines.push({ role, label: role === 'me' ? 'You' : 'Other', text, partial: true });
    }
  }
  return lines;
}

export type RecognitionActivity = 'idle' | 'listening' | 'recognizing' | 'paused';

/** The fallback live signal (path b): while recording & not paused we are always at least
 *  "listening"; "recognizing" once the user is speaking (mic level) or an interim is in flight. */
export function recognitionActivity(opts: {
  recording: boolean;
  paused: boolean;
  hasInterim: boolean;
  speaking: boolean;
}): RecognitionActivity {
  if (!opts.recording) return 'idle';
  if (opts.paused) return 'paused';
  if (opts.hasInterim || opts.speaking) return 'recognizing';
  return 'listening';
}
