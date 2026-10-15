// A live transcript row can be taken back after it was shown: Deepgram
// realtime shows a Mic-In line at once and retracts it if a matching
// Speaker-Out line later proves it was the far side leaking into the mic.
//
// DO NOT change this name here alone: it MUST equal TRANSCRIPT_RETRACT_EVENT in
// src-tauri/src/audio/deepgram.rs (`ts_listens_for_transcript_retract` checks).
export const TRANSCRIPT_RETRACT_EVENT = 'transcript-retract';

export interface TranscriptRetract {
  sequence_id: number;
}

/** The rows left after retracting `sequenceId` (same array if absent). */
export function withoutRetracted<T extends { sequence_id?: number }>(rows: T[], sequenceId: number): T[] {
  return rows.some((row) => row.sequence_id === sequenceId)
    ? rows.filter((row) => row.sequence_id !== sequenceId)
    : rows;
}

/**
 * Rows after a reload sync: the backend history plus any live rows that
 * arrived while it was being fetched, minus rows retracted meanwhile.
 * Replacing state with the history alone would drop those live rows and
 * bring back a line retracted during the fetch.
 */
export function mergeHistory<T extends { sequence_id?: number; chunk_start_time?: number }>(
  history: T[],
  current: T[],
  retracted: ReadonlySet<number>,
): T[] {
  const seen = new Set(history.map((row) => row.sequence_id));
  // Same order as the live transcript: audio time, then arrival.
  return [...history, ...current.filter((row) => !seen.has(row.sequence_id))]
    .filter((row) => row.sequence_id === undefined || !retracted.has(row.sequence_id))
    .sort((a, b) => ((a.chunk_start_time ?? 0) - (b.chunk_start_time ?? 0)) || ((a.sequence_id ?? 0) - (b.sequence_id ?? 0)));
}
