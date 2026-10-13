import { describe, expect, mock, test } from 'bun:test';
import { createNotesAutosaver, type NotesSaveState } from '../../src/lib/notes-autosave';
import type { NotesContent } from '../../src/lib/meeting-notes';

const note = (markdown: string): NotesContent => ({ blocks: [] as any, markdown });
const wait = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

describe('createNotesAutosaver', () => {
  test('saves once per burst of edits, with the newest content', async () => {
    const save = mock(async (_content: NotesContent) => {});
    const autosaver = createNotesAutosaver({ save, delayMs: 20 });

    autosaver.schedule(note('a'));
    autosaver.schedule(note('ab'));
    autosaver.schedule(note('abc'));
    await wait(60);

    expect(save).toHaveBeenCalledTimes(1);
    expect(save.mock.calls[0][0].markdown).toBe('abc');
    expect(autosaver.latest()?.markdown).toBe('abc');
  });

  test('flush saves pending edits immediately', async () => {
    const save = mock(async (_content: NotesContent) => {});
    const autosaver = createNotesAutosaver({ save, delayMs: 10_000 });

    autosaver.schedule(note('typed'));
    await autosaver.flush();

    expect(save).toHaveBeenCalledTimes(1);
  });

  test('flush with nothing pending does not save', async () => {
    const save = mock(async (_content: NotesContent) => {});
    const autosaver = createNotesAutosaver({ save, delayMs: 20 });

    await autosaver.flush();

    expect(save).not.toHaveBeenCalled();
  });

  test('a failed save reports an error and retry saves the same edit', async () => {
    const states: NotesSaveState[] = [];
    let fail = true;
    const save = mock(async (_content: NotesContent) => {
      if (fail) throw new Error('disk full');
    });
    const consoleError = console.error;
    console.error = () => {};
    const autosaver = createNotesAutosaver({ save, delayMs: 10_000, onStateChange: (s) => states.push(s) });

    autosaver.schedule(note('keep me'));
    await autosaver.flush();
    expect(states).toEqual(['saving', 'error']);

    fail = false;
    await autosaver.retry();
    console.error = consoleError;

    expect(save).toHaveBeenCalledTimes(2);
    expect(save.mock.calls[1][0].markdown).toBe('keep me');
    expect(states).toEqual(['saving', 'error', 'saving', 'saved']);
  });

  test('an edit made during a save is saved afterwards, in order', async () => {
    const saved: string[] = [];
    let releaseFirst!: () => void;
    const save = mock(async (content: NotesContent) => {
      if (content.markdown === 'first') await new Promise<void>((resolve) => { releaseFirst = resolve; });
      saved.push(content.markdown);
    });
    const autosaver = createNotesAutosaver({ save, delayMs: 10_000 });

    autosaver.schedule(note('first'));
    const firstSave = autosaver.flush();
    await wait(0);
    autosaver.schedule(note('second'));
    const secondSave = autosaver.flush();
    releaseFirst();
    await Promise.all([firstSave, secondSave]);

    expect(saved).toEqual(['first', 'second']);
  });

  test('hasUnsaved is true until an edit is saved, and stays true after a failed save', async () => {
    let fail = false;
    const save = mock(async (_content: NotesContent) => {
      if (fail) throw new Error('disk full');
    });
    const consoleError = console.error;
    console.error = () => {};
    try {
      const autosaver = createNotesAutosaver({ save, delayMs: 10_000 });
      expect(autosaver.hasUnsaved()).toBe(false);

      autosaver.schedule(note('a'));
      expect(autosaver.hasUnsaved()).toBe(true);
      await autosaver.flush();
      expect(autosaver.hasUnsaved()).toBe(false);

      fail = true;
      autosaver.schedule(note('b'));
      await autosaver.flush();
      expect(autosaver.hasUnsaved()).toBe(true);
    } finally {
      console.error = consoleError;
    }
  });
});
