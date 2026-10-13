import { afterAll, afterEach, beforeEach, describe, expect, mock, test } from 'bun:test';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';

const originalCore = { ...await import('@tauri-apps/api/core') };
afterAll(() => {
  mock.module('@tauri-apps/api/core', () => originalCore);
});

type Stored = { markdown: string; notes_json: string | null } | null;
let readNotes: (meetingId: string) => Promise<Stored>;
const saves: Array<Record<string, unknown>> = [];
const invoke = mock(async (command: string, args?: Record<string, unknown>): Promise<unknown> => {
  if (command === 'api_get_meeting_notes') return readNotes(args!.meetingId as string);
  if (command === 'api_save_meeting_notes') {
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

    expect(saves.length).toBe(1);
  });

  test('a failed load reports an error and reload tries again', async () => {
    const consoleError = console.error;
    console.error = () => {};
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
    console.error = consoleError;

    expect(notes.loadState).toBe('ready');
    expect(notes.initialMarkdown).toBe('Recovered');
  });

  test('getMarkdown returns the newest edit, saved or not', async () => {
    readNotes = async () => ({ markdown: 'stored', notes_json: null });
    await show('meeting-a');
    await settle();

    act(() => { notes.handleChange({ blocks: [block('newer')] as any, markdown: 'newer' }); });

    expect(notes.getMarkdown()).toBe('newer');
  });
});
