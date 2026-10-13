import type { Block } from '@blocknote/core';
import type { NotesContent } from '@/lib/meeting-notes';
import type { NotesDraft } from '@/services/indexedDBService';
import { notesService } from '@/services/notesService';

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
