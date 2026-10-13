import { useCallback, useEffect, useMemo } from 'react';
import { useTranscripts } from '@/contexts/TranscriptContext';
import type { NotesContent } from '@/lib/meeting-notes';
import { createNotesAutosaver } from '@/lib/notes-autosave';
import { indexedDBService } from '@/services/indexedDBService';
import { recordingNotesDraft } from '@/services/recordingNotesDraft';

/**
 * Notes for the recording in progress. Edits go to the in-memory draft at once
 * and to the crash-recovery record shortly after.
 */
export function useRecordingNotesDraft(persistDelayMs = 500) {
  const { currentMeetingId: recordingId } = useTranscripts();

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

  const stored = recordingNotesDraft.get(recordingId);
  return {
    recordingId,
    initialBlocks: stored?.blocks ?? null,
    initialMarkdown: stored?.markdown ?? '',
    handleChange,
  };
}
