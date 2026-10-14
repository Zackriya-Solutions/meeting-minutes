import { afterAll, afterEach, beforeEach, describe, expect, mock, test } from 'bun:test';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';
import { withQuietConsoleError } from '../helpers/quiet-console';

const originalCore = { ...await import('@tauri-apps/api/core') };
const originalToast = { ...await import('sonner') };
afterAll(() => {
  mock.module('@tauri-apps/api/core', () => originalCore);
  mock.module('sonner', () => originalToast);
});

type Stored = { markdown: string; notes_json: string | null } | null;
let readNotes: (meetingId: string) => Promise<Stored>;
const saves: Array<Record<string, unknown>> = [];
let failSaves = 0;
const toastError = mock((..._args: unknown[]) => {});
mock.module('sonner', () => ({ toast: { error: toastError } }));
const invoke = mock(async (command: string, args?: Record<string, unknown>): Promise<unknown> => {
  if (command === 'api_get_meeting_notes') return readNotes(args!.meetingId as string);
  if (command === 'api_save_meeting_notes') {
    if (failSaves > 0) {
      failSaves -= 1;
      throw new Error('database locked');
    }
    saves.push(args!);
    return null;
  }
  throw new Error(`Unexpected command: ${command}`);
});
mock.module('@tauri-apps/api/core', () => ({ ...originalCore, invoke }));
const { useMeetingNotes } = await import('../../src/hooks/meeting-details/useMeetingNotes');

let notes: ReturnType<typeof useMeetingNotes>;
function Harness({ meetingId }: { meetingId: string }) {
  notes = useMeetingNotes(meetingId, 10_000);
  return null;
}

const block = (text: string) => ({
  id: text,
  type: 'paragraph',
  props: {},
  content: [{ type: 'text', text, styles: {} }],
  children: [],
});
const settle = () => act(async () => { await new Promise((resolve) => setTimeout(resolve, 0)); });

let renderer: ReactTestRenderer | undefined;
async function show(meetingId: string) {
  await act(async () => {
    const view = <Harness meetingId={meetingId} />;
    if (renderer) renderer.update(view); else renderer = create(view);
  });
}

beforeEach(() => {
  saves.length = 0;
  failSaves = 0;
  toastError.mockClear();
  readNotes = async () => null;
});
afterEach(async () => {
  if (renderer) await act(async () => renderer!.unmount());
  renderer = undefined;
});

describe('useMeetingNotes', () => {
  test('loads the stored note for the meeting', async () => {
    readNotes = async () => ({ markdown: 'Ship Friday', notes_json: JSON.stringify([block('Ship Friday')]) });

    await show('meeting-a');
    await settle();

    expect(notes.loadState).toBe('ready');
    expect(notes.initialMarkdown).toBe('Ship Friday');
    expect(notes.initialBlocks as unknown).toEqual([block('Ship Friday')]);
    expect(notes.getMarkdown()).toBe('Ship Friday');
  });

  test("never exposes the previous meeting's note while the next meeting loads", async () => {
    let releaseB!: (stored: Stored) => void;
    readNotes = (meetingId) =>
      meetingId === 'meeting-a'
        ? Promise.resolve({ markdown: 'A notes', notes_json: JSON.stringify([block('A notes')]) })
        : new Promise<Stored>((resolve) => { releaseB = resolve; });
    await show('meeting-a');
    await settle();
    expect(notes.initialMarkdown).toBe('A notes');

    await show('meeting-b');

    expect(notes.loadState).toBe('loading');
    expect(notes.initialBlocks).toBeNull();
    expect(notes.initialMarkdown).toBe('');

    await act(async () => { releaseB({ markdown: 'B notes', notes_json: null }); });
    await settle();
    expect(notes.loadState).toBe('ready');
    expect(notes.initialMarkdown).toBe('B notes');
    expect(notes.initialBlocks).toBeNull();
  });

  test('an edit is saved to the meeting it was typed in when the user switches meeting', async () => {
    await show('meeting-a');
    await settle();
    act(() => { notes.handleChange({ blocks: [block('for a')] as any, markdown: 'for a' }); });

    await show('meeting-b');
    await settle();

    expect(saves).toEqual([
      { meetingId: 'meeting-a', markdown: 'for a', notesJson: JSON.stringify([block('for a')]) },
    ]);
    expect(notes.saveState).toBe('idle');
  });

  test('pending edits are saved when the view closes', async () => {
    await show('meeting-a');
    await settle();
    act(() => { notes.handleChange({ blocks: [block('typed')] as any, markdown: 'typed' }); });

    await act(async () => renderer!.unmount());
    renderer = undefined;
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(saves).toEqual([
      { meetingId: 'meeting-a', markdown: 'typed', notesJson: JSON.stringify([block('typed')]) },
    ]);
  });

  test('a failed load reports an error and reload tries again', async () => {
    await withQuietConsoleError(async () => {
      let attempts = 0;
      readNotes = async () => {
        attempts += 1;
        if (attempts === 1) throw new Error('database locked');
        return { markdown: 'Recovered', notes_json: null };
      };
      await show('meeting-a');
      await settle();
      expect(notes.loadState).toBe('error');

      act(() => { notes.reload(); });
      await settle();

      expect(notes.loadState).toBe('ready');
      expect(notes.initialMarkdown).toBe('Recovered');
    });
  });

  test('reload saves a pending edit before it loads again', async () => {
    const order: string[] = [];
    readNotes = async () => {
      order.push('get');
      return null;
    };
    await show('meeting-a');
    await settle();
    act(() => { notes.handleChange({ blocks: [block('pending')] as any, markdown: 'pending' }); });

    act(() => { notes.reload(); });
    await settle();

    expect(saves).toEqual([
      { meetingId: 'meeting-a', markdown: 'pending', notesJson: JSON.stringify([block('pending')]) },
    ]);
    expect(order).toEqual(['get', 'get']);
    const saveIndex = invoke.mock.calls.map((c) => c[0]).lastIndexOf('api_save_meeting_notes');
    const lastGet = invoke.mock.calls.map((c) => c[0]).lastIndexOf('api_get_meeting_notes');
    expect(saveIndex).toBeLessThan(lastGet);
  });

  test('reload does not load again while the pending edit could not be saved', async () => {
    await withQuietConsoleError(async () => {
      const gets = () => invoke.mock.calls.filter((c) => c[0] === 'api_get_meeting_notes').length;
      await show('meeting-a');
      await settle();
      act(() => { notes.handleChange({ blocks: [block('pending')] as any, markdown: 'pending' }); });
      failSaves = 100;
      const getsBefore = gets();

      act(() => { notes.reload(); });
      await settle();

      expect(gets()).toBe(getsBefore);
      expect(notes.saveState).toBe('error');

      failSaves = 0;
      await act(async () => { notes.retrySave(); });
      await settle();
      act(() => { notes.reload(); });
      await settle();

      expect(gets()).toBe(getsBefore + 1);
    });
  });

  test('getMarkdown returns the newest edit, saved or not', async () => {
    readNotes = async () => ({ markdown: 'stored', notes_json: null });
    await show('meeting-a');
    await settle();

    act(() => { notes.handleChange({ blocks: [block('newer')] as any, markdown: 'newer' }); });

    expect(notes.getMarkdown()).toBe('newer');
  });

  test('a save that fails once while switching meeting is retried for the original meeting', async () => {
    await withQuietConsoleError(async () => {
      await show('meeting-a');
      await settle();
      act(() => { notes.handleChange({ blocks: [block('for a')] as any, markdown: 'for a' }); });
      failSaves = 1;

      await show('meeting-b');
      await settle();

      expect(saves).toEqual([
        { meetingId: 'meeting-a', markdown: 'for a', notesJson: JSON.stringify([block('for a')]) },
      ]);
      expect(toastError).not.toHaveBeenCalled();
    });
  });

  test('an edit that cannot be saved while switching meeting raises an error notice', async () => {
    await withQuietConsoleError(async () => {
      await show('meeting-a');
      await settle();
      act(() => { notes.handleChange({ blocks: [block('for a')] as any, markdown: 'for a' }); });
      failSaves = 2;

      await show('meeting-b');
      await settle();

      expect(saves).toEqual([]);
      expect(toastError).toHaveBeenCalledTimes(1);
      expect(toastError.mock.calls[0][0]).toBe("Couldn't save notes");
    });
  });

  test('returning to a meeting before the other one loads reloads it instead of reusing old content', async () => {
    let storedA = 'old A';
    let releaseA2!: (stored: Stored) => void;
    let loadsOfA = 0;
    readNotes = (meetingId) => {
      if (meetingId === 'meeting-b') return new Promise<Stored>(() => {});
      loadsOfA += 1;
      if (loadsOfA === 1) return Promise.resolve({ markdown: storedA, notes_json: null });
      return new Promise<Stored>((resolve) => { releaseA2 = resolve; });
    };
    await show('meeting-a');
    await settle();
    expect(notes.loadState).toBe('ready');
    act(() => { notes.handleChange({ blocks: [block('edited A')] as any, markdown: 'edited A' }); });
    storedA = 'edited A';

    await show('meeting-b');
    await settle();
    await show('meeting-a');
    await settle();

    expect(notes.loadState).toBe('loading');
    expect(notes.initialMarkdown).toBe('');

    await act(async () => { releaseA2({ markdown: 'edited A', notes_json: null }); });
    await settle();
    expect(notes.loadState).toBe('ready');
    expect(notes.initialMarkdown).toBe('edited A');
  });
});
