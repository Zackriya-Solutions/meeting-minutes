import type { Block } from '@blocknote/core';
import type { NotesContent } from '@/lib/meeting-notes';
import { indexedDBService, type NotesDraft } from '@/services/indexedDBService';
import { notesService } from '@/services/notesService';
import { notifyUnsavedNotes } from '@/lib/notes-toast';

// A meeting has no database row until its recording is saved, so notes typed
// while recording are held here, tagged with the recording they belong to.
let current: { recordingId: string; content: NotesContent } | null = null;

export const recordingNotesDraft = {
  set(recordingId: string, content: NotesContent): void {
    current = { recordingId, content };
  },

  get(recordingId: string | null): NotesContent | null {
    return current && current.recordingId === recordingId ? current.content : null;
  },

  clear(recordingId: string): void {
    if (current?.recordingId === recordingId) current = null;
  },
};

export type NotesCommitResult =
  | { status: 'none' }
  | { status: 'saved' }
  | { status: 'failed'; markdown: string };

type RecoveryDraftLoader = (recordingId: string) => Promise<NotesDraft | null | undefined>;

const RECORDING_ID_KEY = 'indexeddb_current_meeting_id';

// After a reload the context has no recording id until the next recording event,
// but the id of the recording in progress is still in session storage.
export function storedRecordingId(): string | null {
  try {
    return typeof sessionStorage !== 'undefined' ? sessionStorage.getItem(RECORDING_ID_KEY) : null;
  } catch {
    return null;
  }
}

/** Reads the notes draft kept on the crash-recovery record. */
export const loadRecoveryNotesDraft: RecoveryDraftLoader = async (id) =>
  (await indexedDBService.getMeetingMetadata(id))?.notesDraft;

/**
 * The draft for a recording: the in-memory one, or, after a reload has emptied
 * memory, the one kept on the recovery record. Never throws.
 */
export async function resolveNotesDraft(
  recordingId: string | null,
  loadRecoveryDraft: RecoveryDraftLoader = loadRecoveryNotesDraft,
): Promise<NotesDraft | null> {
  const inMemory = recordingNotesDraft.get(recordingId);
  if (inMemory) return inMemory;
  if (!recordingId) return null;
  try {
    return (await loadRecoveryDraft(recordingId)) ?? null;
  } catch (error) {
    console.error('Failed to read the recording notes draft', error);
    return null;
  }
}

/**
 * Puts the draft kept on the recovery record back into memory after a reload.
 * Returns whether anything was restored. Never throws.
 */
export async function restoreRecordingNotesDraft(
  recordingId: string,
  loadRecoveryDraft: RecoveryDraftLoader = loadRecoveryNotesDraft,
): Promise<boolean> {
  if (recordingNotesDraft.get(recordingId)) return false;
  try {
    const stored = await loadRecoveryDraft(recordingId);
    // Typing may have started while the stored draft was loading.
    if (!stored || !stored.markdown.trim() || recordingNotesDraft.get(recordingId)) return false;
    recordingNotesDraft.set(recordingId, { blocks: stored.blocks as Block[], markdown: stored.markdown });
    return true;
  } catch (error) {
    console.error('Failed to restore the recording notes draft', error);
    return false;
  }
}

/** Writes notes typed during a recording to the meeting that was just saved. */
export async function commitNotesDraft(
  draft: NotesDraft | null | undefined,
  meetingId: string,
): Promise<NotesCommitResult> {
  if (!draft || !draft.markdown.trim()) return { status: 'none' };
  try {
    await notesService.save(meetingId, { blocks: draft.blocks as Block[], markdown: draft.markdown });
    return { status: 'saved' };
  } catch (error) {
    console.error('Failed to save recording notes to the meeting', error);
    return { status: 'failed', markdown: draft.markdown };
  }
}

/**
 * For a stop that ends without a saved meeting: tells the user their notes were
 * not saved, with a way to copy them, and drops the draft. Returns whether
 * there were notes to report. Never throws.
 */
export async function reportUnsavedRecordingNotes(
  recordingId: string | null,
  loadRecoveryDraft: RecoveryDraftLoader = loadRecoveryNotesDraft,
): Promise<boolean> {
  const draft = await resolveNotesDraft(recordingId, loadRecoveryDraft);
  if (!draft || !draft.markdown.trim()) return false;
  notifyUnsavedNotes({
    level: 'error',
    title: 'Your notes were not saved',
    description: 'Copy them now so they are not lost.',
    markdown: draft.markdown,
  });
  if (recordingId) recordingNotesDraft.clear(recordingId);
  return true;
}

/**
 * Writes the recording's notes draft to the meeting that was just saved and
 * drops the draft. If the write fails, tells the user and keeps the draft.
 */
export async function saveRecordingNotesToMeeting(
  recordingId: string | null,
  meetingId: string,
  loadRecoveryDraft: RecoveryDraftLoader = loadRecoveryNotesDraft,
): Promise<void> {
  const notesDraft = await resolveNotesDraft(recordingId, loadRecoveryDraft);
  const notesResult = await commitNotesDraft(notesDraft, meetingId);
  if (notesResult.status === 'saved' && recordingId) {
    recordingNotesDraft.clear(recordingId);
  } else if (notesResult.status === 'failed') {
    notifyUnsavedNotes({
      level: 'error',
      title: 'Meeting saved, but your notes could not be saved',
      description: 'Copy them now so they are not lost.',
      markdown: notesResult.markdown,
    });
  }
}
