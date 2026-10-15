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
