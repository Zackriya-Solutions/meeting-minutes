import { afterAll, beforeEach, describe, expect, mock, spyOn, test } from 'bun:test';

import { withQuietConsoleError } from '../helpers/quiet-console';

const originalCore = { ...await import('@tauri-apps/api/core') };
afterAll(() => {
  mock.module('@tauri-apps/api/core', () => originalCore);
});

const originalToast = { ...await import('sonner') };
afterAll(() => {
  mock.module('sonner', () => originalToast);
});
const toastError = mock((_title: string, _options?: unknown) => {});
mock.module('sonner', () => ({ toast: { error: toastError, warning: mock(() => {}), success: mock(() => {}) } }));

let failSave = false;
const saves: Array<Record<string, unknown>> = [];
const invoke = mock(async (command: string, args?: Record<string, unknown>): Promise<unknown> => {
  if (command !== 'api_save_meeting_notes') throw new Error(`Unexpected command: ${command}`);
  if (failSave) throw new Error('database locked');
  saves.push(args!);
  return null;
});
mock.module('@tauri-apps/api/core', () => ({ ...originalCore, invoke }));
const { commitNotesDraft, recordingNotesDraft, reportUnsavedRecordingNotes, resolveNotesDraft, restoreRecordingNotesDraft, saveRecordingNotesToMeeting } = await import('../../src/services/recordingNotesDraft');

const blocks = [{ id: 'a', type: 'paragraph', content: [{ type: 'text', text: 'Ship Friday' }], children: [] }];
const draft = { blocks: blocks as any, markdown: 'Ship Friday' };

beforeEach(() => {
  failSave = false;
  toastError.mockClear();
  saves.length = 0;
  recordingNotesDraft.clear('meeting-1');
  recordingNotesDraft.clear('meeting-2');
});

describe('recordingNotesDraft', () => {
  test('returns the draft only for the recording it belongs to', () => {
    recordingNotesDraft.set('meeting-1', draft);

    expect(recordingNotesDraft.get('meeting-1')).toEqual(draft);
    expect(recordingNotesDraft.get('meeting-2')).toBeNull();
    expect(recordingNotesDraft.get(null)).toBeNull();
  });

  test('a draft left from an earlier recording does not appear in the next one', () => {
    recordingNotesDraft.set('meeting-1', draft);
    recordingNotesDraft.set('meeting-2', { blocks: blocks as any, markdown: 'second meeting' });

    expect(recordingNotesDraft.get('meeting-1')).toBeNull();
    expect(recordingNotesDraft.get('meeting-2')?.markdown).toBe('second meeting');
  });

  test('clear only removes the named recording', () => {
    recordingNotesDraft.set('meeting-1', draft);

    recordingNotesDraft.clear('meeting-2');
    expect(recordingNotesDraft.get('meeting-1')).toEqual(draft);

    recordingNotesDraft.clear('meeting-1');
    expect(recordingNotesDraft.get('meeting-1')).toBeNull();
  });
});

describe('commitNotesDraft', () => {
  test('writes the draft to the saved meeting', async () => {
    expect(await commitNotesDraft(draft, 'saved-meeting')).toEqual({ status: 'saved' });
    expect(saves).toEqual([
      { meetingId: 'saved-meeting', markdown: 'Ship Friday', notesJson: JSON.stringify(blocks) },
    ]);
  });

  test('does nothing when there is no draft or it has no text', async () => {
    expect(await commitNotesDraft(null, 'saved-meeting')).toEqual({ status: 'none' });
    expect(await commitNotesDraft(undefined, 'saved-meeting')).toEqual({ status: 'none' });
    expect(await commitNotesDraft({ blocks: blocks as any, markdown: '  ' }, 'saved-meeting')).toEqual({ status: 'none' });
    expect(saves).toEqual([]);
  });

  test('hands the note text back when saving fails, so it can still be copied', async () => {
    failSave = true;

    const result = await withQuietConsoleError(() => commitNotesDraft(draft, 'saved-meeting'));

    expect(result).toEqual({ status: 'failed', markdown: 'Ship Friday' });
  });
});

const stored = { blocks: blocks as any, markdown: 'Ship Friday' };
const silenceConsole = async <T>(run: () => Promise<T>): Promise<T> => {
  const original = { warn: console.warn, error: console.error };
  console.warn = () => {};
  console.error = () => {};
  try {
    return await run();
  } finally {
    console.warn = original.warn;
    console.error = original.error;
  }
};
const rejectingLoader = async () => {
  throw new Error('idb closed');
};

describe('resolveNotesDraft', () => {
  test('the in-memory draft wins and the loader is not needed', async () => {
    recordingNotesDraft.set('meeting-1', { blocks: blocks as any, markdown: 'from memory' });
    const loader = mock(async () => stored);

    expect(await resolveNotesDraft('meeting-1', loader)).toEqual({ blocks: blocks as any, markdown: 'from memory' });
    expect(loader).not.toHaveBeenCalled();
  });

  test('falls back to the stored draft when memory is empty', async () => {
    const loader = mock(async (_id: string) => stored);

    expect(await resolveNotesDraft('meeting-1', loader)).toEqual(stored);
    expect(loader).toHaveBeenCalledWith('meeting-1');
  });

  test('a missing recording id gives null without loading', async () => {
    const loader = mock(async () => stored);

    expect(await resolveNotesDraft(null, loader)).toBeNull();
    expect(loader).not.toHaveBeenCalled();
  });

  test('a loader that rejects gives null', async () => {
    expect(await silenceConsole(() => resolveNotesDraft('meeting-1', rejectingLoader))).toBeNull();
  });
});

describe('restoreRecordingNotesDraft', () => {
  test('puts a stored draft back into memory', async () => {
    expect(await restoreRecordingNotesDraft('meeting-1', async () => stored)).toBe(true);
    expect(recordingNotesDraft.get('meeting-1')).toEqual(stored);
  });

  test('does nothing when memory already has a draft', async () => {
    recordingNotesDraft.set('meeting-1', { blocks: blocks as any, markdown: 'newer' });

    expect(await restoreRecordingNotesDraft('meeting-1', async () => stored)).toBe(false);
    expect(recordingNotesDraft.get('meeting-1')?.markdown).toBe('newer');
  });

  test('does not overwrite a draft typed while the stored one was loading', async () => {
    const result = await restoreRecordingNotesDraft('meeting-1', async () => {
      recordingNotesDraft.set('meeting-1', { blocks: blocks as any, markdown: 'typed meanwhile' });
      return stored;
    });

    expect(result).toBe(false);
    expect(recordingNotesDraft.get('meeting-1')?.markdown).toBe('typed meanwhile');
  });

  test('ignores a blank or missing stored draft', async () => {
    expect(await restoreRecordingNotesDraft('meeting-1', async () => ({ blocks: [], markdown: '  ' }))).toBe(false);
    expect(await restoreRecordingNotesDraft('meeting-1', async () => null)).toBe(false);
    expect(await restoreRecordingNotesDraft('meeting-1', async () => undefined)).toBe(false);
    expect(recordingNotesDraft.get('meeting-1')).toBeNull();
  });

  test('a loader that rejects restores nothing', async () => {
    expect(await silenceConsole(() => restoreRecordingNotesDraft('meeting-1', rejectingLoader))).toBe(false);
    expect(recordingNotesDraft.get('meeting-1')).toBeNull();
  });
});

describe('reportUnsavedRecordingNotes', () => {
  test('warns about in-memory notes and drops them', async () => {
    recordingNotesDraft.set('meeting-1', draft);

    expect(await reportUnsavedRecordingNotes('meeting-1', async () => null)).toBe(true);

    expect(toastError).toHaveBeenCalledTimes(1);
    expect(toastError.mock.calls[0][0]).toBe('Your notes were not saved');
    expect(recordingNotesDraft.get('meeting-1')).toBeNull();
  });

  test('warns about notes kept on the recovery record', async () => {
    expect(await reportUnsavedRecordingNotes('meeting-1', async () => draft)).toBe(true);

    expect(toastError).toHaveBeenCalledTimes(1);
    expect(toastError.mock.calls[0][0]).toBe('Your notes were not saved');
  });

  test('says nothing when there is nothing to lose', async () => {
    const blank = { blocks: [] as any, markdown: '  \n' };
    recordingNotesDraft.set('meeting-2', blank);

    expect(await reportUnsavedRecordingNotes('meeting-1', async () => null)).toBe(false);
    expect(await reportUnsavedRecordingNotes('meeting-1', async () => blank)).toBe(false);
    expect(await reportUnsavedRecordingNotes('meeting-2', async () => null)).toBe(false);
    expect(await reportUnsavedRecordingNotes(null, async () => draft)).toBe(false);
    expect(toastError).not.toHaveBeenCalled();
  });

  test('does not throw when the recovery record cannot be read', async () => {
    const errorSpy = spyOn(console, 'error').mockImplementation(() => {});
    const result = await reportUnsavedRecordingNotes('meeting-1', async () => {
      throw new Error('idb closed');
    });
    errorSpy.mockRestore();

    expect(result).toBe(false);
    expect(toastError).not.toHaveBeenCalled();
  });
});

describe('saveRecordingNotesToMeeting', () => {
  test('writes the in-memory draft to the meeting and clears it', async () => {
    recordingNotesDraft.set('meeting-1', draft);

    await saveRecordingNotesToMeeting('meeting-1', 'saved-meeting', async () => null);

    expect(saves).toEqual([
      { meetingId: 'saved-meeting', markdown: 'Ship Friday', notesJson: JSON.stringify(blocks) },
    ]);
    expect(recordingNotesDraft.get('meeting-1')).toBeNull();
    expect(toastError).not.toHaveBeenCalled();
  });

  test('takes the draft from the loader when memory is empty', async () => {
    const loader = mock(async (_id: string) => stored);

    await saveRecordingNotesToMeeting('meeting-1', 'saved-meeting', loader);

    expect(loader).toHaveBeenCalledWith('meeting-1');
    expect(saves).toEqual([
      { meetingId: 'saved-meeting', markdown: 'Ship Friday', notesJson: JSON.stringify(blocks) },
    ]);
  });

  test('saves and shows nothing when there is no draft', async () => {
    await saveRecordingNotesToMeeting('meeting-1', 'saved-meeting', async () => null);
    await saveRecordingNotesToMeeting(null, 'saved-meeting', async () => draft);

    expect(saves).toEqual([]);
    expect(toastError).not.toHaveBeenCalled();
  });

  test('a failed save shows one error and keeps the draft', async () => {
    failSave = true;
    recordingNotesDraft.set('meeting-1', draft);

    await withQuietConsoleError(() => saveRecordingNotesToMeeting('meeting-1', 'saved-meeting', async () => null));

    expect(toastError).toHaveBeenCalledTimes(1);
    expect(toastError.mock.calls[0][0]).toBe('Meeting saved, but your notes could not be saved');
    expect(recordingNotesDraft.get('meeting-1')).toEqual(draft);
  });
});
