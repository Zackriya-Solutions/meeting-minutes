'use client';

import { memo, useCallback, useState } from 'react';
import type { NotesContent } from '@/lib/meeting-notes';
import { NotesPanel } from '@/components/MeetingNotes/NotesPanel';
import { useRecordingNotesDraft } from '@/hooks/useRecordingNotesDraft';
import { recordingNotesDraft } from '@/services/recordingNotesDraft';

interface RecordingNotesProps {
  // False once the recording is stopping, so nothing is typed after the note is saved.
  editable: boolean;
  className?: string;
}

interface RecordingNotesPaneProps extends RecordingNotesProps {
  recordingId: string;
  onChange: (content: NotesContent) => void;
  onRetryLoad: () => void;
}

// The editor's starting content is read once per mount; the page re-renders
// often while recording and must not hand the editor a new document each time.
const RecordingNotesPane = memo(function RecordingNotesPane({ recordingId, onChange, onRetryLoad, editable, className }: RecordingNotesPaneProps) {
  const [initial] = useState(() => recordingNotesDraft.get(recordingId));

  return (
    <NotesPanel
      initialBlocks={initial?.blocks ?? null}
      initialMarkdown={initial?.markdown ?? ''}
      saveState="idle"
      onChange={onChange}
      onRetryLoad={onRetryLoad}
      editable={editable}
      className={className}
    />
  );
});

const ignoreChange = () => {};

/** Notes pane shown beside the live transcript while recording. */
export function RecordingNotes({ editable, className }: RecordingNotesProps) {
  const { recordingId, isReady, handleChange } = useRecordingNotesDraft();
  const [attempt, setAttempt] = useState(0);
  const retryLoad = useCallback(() => setAttempt((current) => current + 1), []);

  if (!recordingId || !isReady) {
    // The id is only legitimately pending while recording starts; it is cleared again as the meeting is saved.
    if (!editable) return null;
    return (
      <NotesPanel
        loadState="loading"
        initialBlocks={null}
        initialMarkdown=""
        saveState="idle"
        onChange={ignoreChange}
        className={className}
      />
    );
  }

  return (
    <RecordingNotesPane
      key={`${recordingId}:${attempt}`}
      recordingId={recordingId}
      onChange={handleChange}
      onRetryLoad={retryLoad}
      editable={editable}
      className={className}
    />
  );
}
