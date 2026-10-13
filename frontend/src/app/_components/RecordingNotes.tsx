'use client';

import { useState } from 'react';
import { NotesPanel } from '@/components/MeetingNotes/NotesPanel';
import { useRecordingNotesDraft } from '@/hooks/useRecordingNotesDraft';

interface RecordingNotesProps {
  // False once the recording is stopping, so nothing is typed after the note is saved.
  editable: boolean;
  className?: string;
}

/** Notes pane shown beside the live transcript while recording. */
export function RecordingNotes({ editable, className }: RecordingNotesProps) {
  const { recordingId, initialBlocks, initialMarkdown, handleChange } = useRecordingNotesDraft();
  const [attempt, setAttempt] = useState(0);

  if (!recordingId) return null;

  return (
    <NotesPanel
      key={`${recordingId}:${attempt}`}
      initialBlocks={initialBlocks}
      initialMarkdown={initialMarkdown}
      saveState="idle"
      onChange={handleChange}
      onRetryLoad={() => setAttempt((current) => current + 1)}
      editable={editable}
      className={className}
    />
  );
}
