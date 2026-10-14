import { afterAll, beforeEach, describe, expect, mock, test } from 'bun:test';

const originalCore = { ...await import('@tauri-apps/api/core') };
afterAll(() => {
  mock.module('@tauri-apps/api/core', () => originalCore);
});

const calls: Array<{ command: string; args: any }> = [];
let releaseSave: (() => void) | null = null;
let holdSaves = false;
const invoke = mock(async (command: string, args?: Record<string, unknown>): Promise<unknown> => {
  if (command === 'api_save_meeting_notes' && holdSaves) {
    await new Promise<void>((resolve) => { releaseSave = resolve; });
  }
  calls.push({ command, args });
  return null;
});
mock.module('@tauri-apps/api/core', () => ({ ...originalCore, invoke }));
const { notesService } = await import('../../src/services/notesService');

const blocks = [{ id: 'a', type: 'paragraph', content: [{ type: 'text', text: 'Ship Friday' }], children: [] }];

beforeEach(() => {
  calls.length = 0;
  holdSaves = false;
  releaseSave = null;
});

describe('notesService', () => {
  test('saves markdown and the block array as JSON', async () => {
    await notesService.save('m1', { blocks: blocks as any, markdown: 'Ship Friday' });

    expect(calls).toEqual([
      {
        command: 'api_save_meeting_notes',
        args: { meetingId: 'm1', markdown: 'Ship Friday', notesJson: JSON.stringify(blocks) },
      },
    ]);
  });

  test('sends no block JSON for an empty note', async () => {
    await notesService.save('m1', { blocks: blocks as any, markdown: '  \n' });

    expect(calls[0].args.notesJson).toBeNull();
  });

  test('a load waits for a save of the same meeting that is still running', async () => {
    holdSaves = true;
    const saving = notesService.save('m1', { blocks: blocks as any, markdown: 'Ship Friday' });
    const loading = notesService.get('m1');
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(calls).toEqual([]);

    releaseSave!();
    await Promise.all([saving, loading]);

    expect(calls.map((c) => c.command)).toEqual(['api_save_meeting_notes', 'api_get_meeting_notes']);
  });
});
