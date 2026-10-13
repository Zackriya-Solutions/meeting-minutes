import type { NotesContent } from './meeting-notes';

export type NotesSaveState = 'idle' | 'saving' | 'saved' | 'error';

interface NotesAutosaverOptions {
  save: (content: NotesContent) => Promise<void>;
  /** How long to wait after the last edit before saving. */
  delayMs: number;
  onStateChange?: (state: NotesSaveState) => void;
}

export interface NotesAutosaver {
  /** Records an edit and (re)starts the wait before saving. */
  schedule: (content: NotesContent) => void;
  /** Saves any pending edit now. Resolves when no save is running. */
  flush: () => Promise<void>;
  /** The same operation as `flush`: a failed save keeps its edit pending, so this saves it again. */
  retry: () => Promise<void>;
  /** The most recent edit, saved or not. */
  latest: () => NotesContent | null;
  /** True when an edit has not been saved yet (still waiting, or its save failed). */
  hasUnsaved: () => boolean;
}

export function createNotesAutosaver({ save, delayMs, onStateChange }: NotesAutosaverOptions): NotesAutosaver {
  let pending: NotesContent | null = null;
  let latest: NotesContent | null = null;
  let timer: ReturnType<typeof setTimeout> | null = null;
  let running: Promise<void> = Promise.resolve();

  const run = (): Promise<void> => {
    if (timer) {
      clearTimeout(timer);
      timer = null;
    }
    running = running.then(async () => {
      if (!pending) return;
      const content = pending;
      pending = null;
      onStateChange?.('saving');
      try {
        await save(content);
        if (!pending) onStateChange?.('saved');
      } catch (error) {
        console.error('Failed to save notes', error);
        // Keep the edit so the next attempt saves it, unless a newer one replaced it.
        if (!pending) pending = content;
        onStateChange?.('error');
      }
    });
    return running;
  };

  return {
    schedule(content) {
      latest = content;
      pending = content;
      if (timer) clearTimeout(timer);
      timer = setTimeout(() => {
        void run();
      }, delayMs);
    },
    flush: run,
    retry: run,
    latest: () => latest,
    hasUnsaved: () => pending !== null,
  };
}
