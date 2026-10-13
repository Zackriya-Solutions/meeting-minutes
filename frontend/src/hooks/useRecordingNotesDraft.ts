import { useCallback, useEffect, useMemo, useState } from 'react';
import { useTranscripts } from '@/contexts/TranscriptContext';
import type { NotesContent } from '@/lib/meeting-notes';
import { createNotesAutosaver } from '@/lib/notes-autosave';
import { indexedDBService } from '@/services/indexedDBService';
import { recordingNotesDraft, restoreRecordingNotesDraft, storedRecordingId } from '@/services/recordingNotesDraft';

/**
 * Notes for the recording in progress. Edits go to the in-memory draft at once
 * and to the crash-recovery record shortly after. `isReady` turns true once a
 * draft kept from before a reload has been put back in memory.
 */
export function useRecordingNotesDraft(persistDelayMs = 500) {
  const { currentMeetingId } = useTranscripts();
  const recordingId = currentMeetingId ?? storedRecordingId();
  const [readyFor, setReadyFor] = useState<string | null>(null);

  useEffect(() => {
    if (!recordingId) return;
    let cancelled = false;
    void restoreRecordingNotesDraft(recordingId).then(() => {
      if (!cancelled) setReadyFor(recordingId);
    });
    return () => {
      cancelled = true;
    };
  }, [recordingId]);

  const autosaver = useMemo(
    () =>
      createNotesAutosaver({
        save: async (content) => {
          if (recordingId) await indexedDBService.saveNotesDraft(recordingId, content);
        },
        delayMs: persistDelayMs,
      }),
    [recordingId, persistDelayMs],
  );

  useEffect(
    () => () => {
      void autosaver.flush();
    },
    [autosaver],
  );

  const handleChange = useCallback(
    (content: NotesContent) => {
      if (!recordingId) return;
      recordingNotesDraft.set(recordingId, content);
      autosaver.schedule(content);
    },
    [recordingId, autosaver],
  );

  return { recordingId, isReady: recordingId !== null && readyFor === recordingId, handleChange };
}
