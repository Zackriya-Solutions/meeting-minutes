export type RecordingPersistenceDecision =
  | 'save'
  | 'skip'
  | 'preserve_for_recovery';

/**
 * A completed transcription can be committed as a normal meeting. If native stop was
 * requested but transcription never settled, keep the temporary IndexedDB/native files
 * recoverable instead of presenting a partial transcript as a completed meeting.
 */
export function decideRecordingPersistence(
  shouldSave: boolean,
  transcriptionComplete: boolean,
): RecordingPersistenceDecision {
  if (!shouldSave) return 'skip';
  return transcriptionComplete ? 'save' : 'preserve_for_recovery';
}
