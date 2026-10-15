import { afterAll, afterEach, beforeEach, describe, expect, mock, test } from 'bun:test';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';
import type { MeetingSummaryContext } from '../../src/lib/summary-context';

// Bun shares module mocks between test files; restore the real modules after this suite.
const originalCore = { ...await import('@tauri-apps/api/core') };
const originalToast = { ...await import('sonner') };
afterAll(() => {
  mock.module('@tauri-apps/api/core', () => originalCore);
  mock.module('sonner', () => originalToast);
});

const toastError = mock((_message: string, _options?: unknown) => {});
mock.module('sonner', () => ({ toast: { error: toastError, info() {}, success() {}, warning() {} } }));

type SaveArgs = { meetingId: string; context: string; useInherited: boolean };
let load: (meetingId: string) => Promise<MeetingSummaryContext>;
let save: (args: SaveArgs) => Promise<void>;
const saved: SaveArgs[] = [];
const invoke = mock(async (command: string, args?: Record<string, unknown>): Promise<unknown> => {
  if (command === 'api_get_meeting_summary_context') return load(args!.meetingId as string);
  if (command === 'api_save_meeting_summary_context') {
    saved.push(args as SaveArgs);
    return save(args as SaveArgs);
  }
  throw new Error(`Unexpected command: ${command}`);
});
mock.module('@tauri-apps/api/core', () => ({ ...originalCore, invoke }));
const { useSummaryContext } = await import('../../src/hooks/meeting-details/useSummaryContext');

const DELAY = 20;
const context = (overrides: Partial<MeetingSummaryContext> = {}): MeetingSummaryContext => ({
  context: '', useInherited: true, inherited: [], ...overrides,
});

let state: ReturnType<typeof useSummaryContext>;
function Probe({ meetingId }: { meetingId: string }) {
  state = useSummaryContext(meetingId, DELAY);
  return null;
}

let renderer: ReactTestRenderer;
const settle = (ms = 0) => act(async () => { await new Promise(resolve => setTimeout(resolve, ms)); });
// Renders, then waits one timer turn so the load's promise chain has settled.
async function show(meetingId: string) {
  await act(async () => {
    const view = <Probe meetingId={meetingId} />;
    if (renderer) renderer.update(view); else renderer = create(view);
  });
  await settle();
}

beforeEach(() => {
  renderer = undefined as unknown as ReactTestRenderer;
  saved.length = 0;
  toastError.mockClear();
  load = async () => context();
  save = async () => {};
});
afterEach(async () => {
  if (renderer) await act(async () => renderer.unmount());
});

describe('summary context for the open meeting', () => {
  test('loads the saved text, switch and inherited levels', async () => {
    const global = { scope: 'global' as const, scopeId: '', label: null, context: 'I am Noah.' };
    load = async () => context({ context: 'Saved text', useInherited: false, inherited: [global] });
    await show('meeting-a');
    expect(state.isLoaded).toBe(true);
    expect(state.text).toBe('Saved text');
    expect(state.useInherited).toBe(false);
    expect(state.inherited).toEqual([global]);
  });

  test('is not loaded until the load has finished', async () => {
    let resolve!: (value: MeetingSummaryContext) => void;
    load = () => new Promise(done => { resolve = done; });
    await show('meeting-a');
    expect(state.isLoaded).toBe(false);
    resolve(context({ context: 'Late' }));
    await settle();
    expect(state.isLoaded).toBe(true);
    expect(state.text).toBe('Late');
  });

  test('a late load for the previous meeting is ignored', async () => {
    let resolveA!: (value: MeetingSummaryContext) => void;
    load = meetingId => meetingId === 'meeting-a'
      ? new Promise(done => { resolveA = done; })
      : Promise.resolve(context({ context: 'B text' }));
    await show('meeting-a');
    await show('meeting-b');
    resolveA(context({ context: 'A text' }));
    await settle();
    expect(state.text).toBe('B text');
  });

  test('typing saves once after the quiet period', async () => {
    await show('meeting-a');
    await act(async () => { state.setText('h'); state.setText('hello'); });
    expect(state.text).toBe('hello');
    expect(saved).toEqual([]);
    await settle(DELAY * 4);
    expect(saved).toEqual([{ meetingId: 'meeting-a', context: 'hello', useInherited: true }]);
  });

  test('switching meetings saves pending text to the meeting it was typed in', async () => {
    load = async meetingId => context({ context: meetingId === 'meeting-b' ? 'B text' : '' });
    await show('meeting-a');
    await act(async () => { state.setText('typed in A'); });
    await show('meeting-b');
    expect(saved).toEqual([{ meetingId: 'meeting-a', context: 'typed in A', useInherited: true }]);
    expect(state.text).toBe('B text');
    await settle(DELAY * 4);
    expect(saved).toHaveLength(1);
  });

  test('the switch saves immediately, together with the current text', async () => {
    await show('meeting-a');
    await act(async () => { state.setText('note'); state.setUseInherited(false); });
    await settle();
    expect(state.useInherited).toBe(false);
    expect(saved).toEqual([{ meetingId: 'meeting-a', context: 'note', useInherited: false }]);
  });

  test('flush writes pending text before it resolves', async () => {
    await show('meeting-a');
    await act(async () => { state.setText('about to generate'); });
    await act(async () => { await state.flush(); });
    expect(saved).toEqual([{ meetingId: 'meeting-a', context: 'about to generate', useInherited: true }]);
  });

  test('leaving the meeting saves pending text', async () => {
    await show('meeting-a');
    await act(async () => { state.setText('typed then left'); });
    await act(async () => renderer.unmount());
    renderer = undefined as unknown as ReactTestRenderer;
    await settle();
    expect(saved).toEqual([{ meetingId: 'meeting-a', context: 'typed then left', useInherited: true }]);
  });

  test('a failed save shows a toast and keeps the text on screen', async () => {
    save = async () => { throw new Error('disk full'); };
    await show('meeting-a');
    await act(async () => { state.setText('still here'); });
    await act(async () => { await state.flush(); });
    expect(toastError.mock.calls[0]?.[0]).toBe('Failed to save summary context');
    expect(state.text).toBe('still here');
  });

  test('a failed load shows a toast and leaves an empty, usable box', async () => {
    load = async () => { throw new Error('database locked'); };
    await show('meeting-a');
    expect(toastError.mock.calls[0]?.[0]).toBe('Failed to load summary context');
    expect(state.isLoaded).toBe(true);
    expect(state.text).toBe('');
    expect(state.inherited).toEqual([]);
    await act(async () => { state.setText('typed after failure'); });
    await act(async () => { await state.flush(); });
    expect(saved).toEqual([{ meetingId: 'meeting-a', context: 'typed after failure', useInherited: true }]);
  });

  test('edits before the load has finished are ignored and save nothing', async () => {
    let resolve!: (value: MeetingSummaryContext) => void;
    load = () => new Promise(done => { resolve = done; });
    await show('meeting-a');
    await act(async () => { state.setText('too early'); state.setUseInherited(false); });
    await settle(DELAY * 4);
    expect(saved).toEqual([]);
    resolve(context({ context: 'Saved text' }));
    await settle();
    expect(state.text).toBe('Saved text');
    expect(state.useInherited).toBe(true);
  });

  test('after switching to a meeting whose load is pending, edits are ignored too', async () => {
    let resolveB!: (value: MeetingSummaryContext) => void;
    load = meetingId => meetingId === 'meeting-b'
      ? new Promise(done => { resolveB = done; })
      : Promise.resolve(context());
    await show('meeting-a');
    await show('meeting-b');
    await act(async () => { state.setText('too early'); });
    await settle(DELAY * 4);
    expect(saved).toEqual([]);
    resolveB(context());
    await settle();
  });
});
