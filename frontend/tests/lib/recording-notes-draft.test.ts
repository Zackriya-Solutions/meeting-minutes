import { afterAll, beforeEach, describe, expect, mock, test } from 'bun:test';

const originalCore = { ...await import('@tauri-apps/api/core') };
afterAll(() => {
  mock.module('@tauri-apps/api/core', () => originalCore);
});

let failSave = false;
const saves: Array<Record<string, unknown>> = [];
const invoke = mock(async (command: string, args?: Record<string, unknown>): Promise<unknown> => {
  if (command !== 'api_save_meeting_notes') throw new Error(`Unexpected command: ${command}`);
  if (failSave) throw new Error('database locked');
  saves.push(args!);
  return null;
});
mock.module('@tauri-apps/api/core', () => ({ ...originalCore, invoke }));
const { commitNotesDraft, recordingNotesDraft } = await import('../../src/services/recordingNotesDraft');

const blocks = [{ id: 'a', type: 'paragraph', content: [{ type: 'text', text: 'Ship Friday' }], children: [] }];
const draft = { blocks: blocks as any, markdown: 'Ship Friday' };

beforeEach(() => {
  failSave = false;
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
    const consoleError = console.error;
    console.error = () => {};

    const result = await commitNotesDraft(draft, 'saved-meeting');
    console.error = consoleError;

    expect(result).toEqual({ status: 'failed', markdown: 'Ship Friday' });
  });
});
