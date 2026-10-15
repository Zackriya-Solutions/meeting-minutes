import { invoke } from '@tauri-apps/api/core';
import type { NotesContent } from '@/lib/meeting-notes';

export interface StoredMeetingNotes {
  markdown: string;
  notes_json: string | null;
}

// Saves still on their way to the database, so a load right after an edit sees it.
const pendingSaves = new Map<string, Promise<void>>();

export const notesService = {
  async get(meetingId: string): Promise<StoredMeetingNotes | null> {
    await pendingSaves.get(meetingId)?.catch(() => undefined);
    return invoke<StoredMeetingNotes | null>('api_get_meeting_notes', { meetingId });
  },

  save(meetingId: string, content: NotesContent): Promise<void> {
    const request = invoke<void>('api_save_meeting_notes', {
      meetingId,
      markdown: content.markdown,
      notesJson: content.markdown.trim() ? JSON.stringify(content.blocks) : null,
    });
    pendingSaves.set(meetingId, request);
    const forget = () => {
      if (pendingSaves.get(meetingId) === request) pendingSaves.delete(meetingId);
    };
    request.then(forget, forget);
    return request;
  },
};
