// VALUEOS: live-transcript state model. The engine currently emits one update per completed
// utterance (no true interim stream — see FEATURE-live-partial-transcript.md), so at runtime
// `deriveLive` (snapshot over the segment array) is used. `reduceLive` is the forward-compatible
// event-stream reducer: it handles a stream of INTERIM updates (latest replaces prior, never
// appended) and commits the FINAL exactly once — so the moment the engine gains a real interim
// stream, wiring it here gives live in-progress text with zero UI changes.
//
// INTERIM IS DISPLAY-ONLY. Only committed (final) segments flow into the enriched export/upload
// (see transcriptFormat.ts, which drops is_partial segments).

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
