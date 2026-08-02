import type { Transcript } from '@/types';

/**
 * Merge incoming transcript updates into the current list, keyed by `sequence_id`.
 *
 * Chunk providers (Whisper, Parakeet) emit each segment once under a fresh id, so
 * merging only ever appends. A streaming provider instead refines a segment **in
 * place**: it re-sends the same id with longer text as the speaker talks, and once
 * more when the segment finalizes. So a repeated id is an update, not a duplicate
 * to discard.
 *
 * Returns the previous array unchanged when nothing actually differs, so React can
 * skip the re-render — streaming providers re-send identical text often.
 */
export function mergeTranscripts(
  prev: Transcript[],
  incoming: Transcript[],
): Transcript[] {
  if (incoming.length === 0) return prev;

  const bySeq = new Map<number, Transcript>();
  // Entries without a sequence_id can't be keyed, so they're carried through
  // untouched and appended (they only appear in loaded/legacy history).
  const seqless: Transcript[] = [];
  for (const t of prev) {
    if (t.sequence_id === undefined) seqless.push(t);
    else bySeq.set(t.sequence_id, t);
  }

  let changed = false;
  for (const t of incoming) {
    if (t.sequence_id === undefined) {
      seqless.push(t);
      changed = true;
      continue;
    }
    const existing = bySeq.get(t.sequence_id);
    if (!existing) {
      bySeq.set(t.sequence_id, t);
      changed = true;
      continue;
    }
    if (existing.text === t.text && existing.is_partial === t.is_partial) {
      continue; // identical resend — leave the object identity alone
    }
    // Keep the original id and start time: this is the same segment being
    // refined, and replacing them would make React remount the row mid-sentence.
    bySeq.set(t.sequence_id, {
      ...existing,
      text: t.text,
      is_partial: t.is_partial,
      confidence: t.confidence,
      audio_end_time: t.audio_end_time,
      duration: t.duration,
    });
    changed = true;
  }

  if (!changed) return prev;

  return [...bySeq.values(), ...seqless].sort(compareTranscripts);
}

/** Chronological order: by chunk start time, then by sequence id. */
export function compareTranscripts(a: Transcript, b: Transcript): number {
  const chunkTimeDiff = (a.chunk_start_time || 0) - (b.chunk_start_time || 0);
  if (chunkTimeDiff !== 0) return chunkTimeDiff;
  return (a.sequence_id || 0) - (b.sequence_id || 0);
}
