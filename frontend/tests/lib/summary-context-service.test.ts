import { afterAll, beforeEach, describe, expect, mock, test } from 'bun:test';

// Bun shares module mocks between test files; restore the real module after this suite.
const originalCore = { ...await import('@tauri-apps/api/core') };
afterAll(() => mock.module('@tauri-apps/api/core', () => originalCore));

let reply: unknown;
const invoke = mock(async (_command: string, _args?: Record<string, unknown>): Promise<unknown> => reply);
mock.module('@tauri-apps/api/core', () => ({ ...originalCore, invoke }));
const { summaryContextService } = await import('../../src/services/summaryContextService');

beforeEach(() => {
  invoke.mockClear();
  reply = undefined;
});

describe('summary context service', () => {
  test('meeting context uses the command and argument names the backend expects', async () => {
    reply = { context: 'saved', useInherited: false, inherited: [] };
    expect(await summaryContextService.getMeeting('m1')).toEqual(reply as never);
    await summaryContextService.saveMeeting('m1', 'text', false);
    expect(invoke.mock.calls).toEqual([
      ['api_get_meeting_summary_context', { meetingId: 'm1' }],
      ['api_save_meeting_summary_context', { meetingId: 'm1', context: 'text', useInherited: false }],
    ]);
  });

  test('global context is read as plain text', async () => {
    reply = { context: 'I am Noah.' };
    expect(await summaryContextService.getGlobal()).toBe('I am Noah.');
    await summaryContextService.saveGlobal('new text');
    expect(invoke.mock.calls).toEqual([
      ['api_get_global_summary_context'],
      ['api_save_global_summary_context', { context: 'new text' }],
    ]);
  });
});
