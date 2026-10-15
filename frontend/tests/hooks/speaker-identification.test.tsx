import { afterAll, afterEach, beforeEach, describe, expect, mock, test } from 'bun:test';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';
import type { SpeakerJobComplete } from '../../src/types';

const originalCore = { ...await import('@tauri-apps/api/core') };
const originalEvent = { ...await import('@tauri-apps/api/event') };
const originalToast = { ...await import('sonner') };
afterAll(() => {
  mock.module('@tauri-apps/api/core', () => originalCore);
  mock.module('@tauri-apps/api/event', () => originalEvent);
  mock.module('sonner', () => originalToast);
});

type Handler = (event: { payload: unknown }) => unknown;
const handlers = new Map<string, Handler>();
const listen = mock(async (name: string, handler: Handler) => {
  handlers.set(name, handler);
  return () => { handlers.delete(name); };
});
mock.module('@tauri-apps/api/event', () => ({ ...originalEvent, listen }));
let guessRefusal: string | null = null;
let guessQueued = true;
const invoke = mock(async (command: string, _args?: Record<string, unknown>): Promise<unknown> => {
  if (command === 'get_speaker_identification_status') return null;
  if (command === 'api_guess_speaker_names') {
    if (guessRefusal) throw guessRefusal;
    return guessQueued;
  }
  throw new Error(`Unexpected command: ${command}`);
});
mock.module('@tauri-apps/api/core', () => ({ ...originalCore, invoke }));
const success = mock((_message: string, _options?: unknown) => {});
const failure = mock((_message: string) => {});
const info = mock((_message: string) => {});
const warning = mock((_message: string, _options?: unknown) => {});
mock.module('sonner', () => ({ toast: { success, error: failure, info, warning } }));
const { useSpeakerIdentification } = await import('../../src/hooks/useSpeakerIdentification');

let state: ReturnType<typeof useSpeakerIdentification>;
const completions: SpeakerJobComplete[] = [];
function View() {
  state = useSpeakerIdentification('meeting-a', (result) => { completions.push(result); });
  return null;
}
let renderer: ReactTestRenderer | undefined;
beforeEach(async () => {
  handlers.clear();
  completions.length = 0;
  guessRefusal = null;
  guessQueued = true;
  invoke.mockClear();
  success.mockClear();
  failure.mockClear();
  info.mockClear();
  warning.mockClear();
  await act(async () => { renderer = create(<View />); });
  // The listeners register asynchronously.
  await act(async () => { await new Promise((resolve) => setTimeout(resolve, 0)); });
});
afterEach(async () => {
  if (renderer) await act(async () => renderer!.unmount());
  renderer = undefined;
});

async function emit(name: string, payload: unknown) {
  await act(async () => { await handlers.get(name)!({ payload }); });
}
const complete = (overrides: Partial<SpeakerJobComplete> = {}): SpeakerJobComplete => ({
  meeting_id: 'meeting-a', kind: 'naming', speaker_count: 4, automatic: false, warning: null, named: 3, suggested: 2, ...overrides,
});

describe('speaker job events', () => {
  test('naming completion toasts result not identified count', async () => {
    await emit('diarization-complete', complete());
    expect(completions).toEqual([complete()]);
    expect(success).toHaveBeenCalledWith('Named 3, suggested 2');
    expect(success.mock.calls.some(([message]) => message.startsWith('Identified'))).toBe(false);
  });

  test('identify completion keeps its toast', async () => {
    await emit('diarization-complete', complete({ kind: 'identify', named: 0, suggested: 0 }));
    expect(success).toHaveBeenCalledWith('Identified 4 speakers');
  });

  test('automatic naming shows no toast', async () => {
    await emit('diarization-complete', complete({ automatic: true }));
    expect(completions).toHaveLength(1);
    expect(success).not.toHaveBeenCalled();
    expect(warning).not.toHaveBeenCalled();
  });

  test('naming error shows reason', async () => {
    await emit('diarization-error', {
      meeting_id: 'meeting-a', kind: 'naming', error: 'No summary model is configured', automatic: false, cancelled: false,
    });
    expect(failure).toHaveBeenCalledWith('No summary model is configured');
    expect(state.isActive).toBe(false);
  });

  test('an automatic guess sends whether a cloud model is allowed', async () => {
    await act(async () => { await state.guessNames(true, true); });
    expect(invoke).toHaveBeenCalledWith('api_guess_speaker_names', { meetingId: 'meeting-a', automatic: true, allowCloud: true });
    await act(async () => { await state.guessNames(true, false); });
    expect(invoke).toHaveBeenCalledWith('api_guess_speaker_names', { meetingId: 'meeting-a', automatic: true, allowCloud: false });
  });

  test('a declined automatic guess ends the pending state at once', async () => {
    // A cloud model without consent: nothing is queued and no event will follow.
    guessQueued = false;
    await act(async () => { await state.guessNames(true, false); });
    expect(state.autoNamingPending).toBe(false);
  });

  test('guess names invokes command and marks job queued', async () => {
    await act(async () => { await state.guessNames(false); });
    expect(invoke).toHaveBeenCalledWith('api_guess_speaker_names', { meetingId: 'meeting-a', automatic: false, allowCloud: null });
    // A guess the user asked for does not hold the auto-summary.
    expect(state.autoNamingPending).toBe(false);
    // The backend emits the queued status before the command returns.
    await emit('diarization-progress', {
      meeting_id: 'meeting-a', kind: 'naming', state: 'queued', stage: null, percent: 0, message: 'Waiting to start…',
    });
    expect(state.isActive).toBe(true);
    expect(state.job?.state).toBe('queued');
    expect(state.job?.kind).toBe('naming');
  });

  test('automatic guess stays pending until its first naming event', async () => {
    await act(async () => { await state.guessNames(true, false); });
    // The command has returned but its queued event has not arrived: nothing is active yet.
    expect(state.isActive).toBe(false);
    expect(state.autoNamingPending).toBe(true);
    await emit('diarization-progress', {
      meeting_id: 'meeting-a', kind: 'identify', state: 'running', stage: 'saving', percent: 95, message: 'Saving speakers…',
    });
    expect(state.autoNamingPending).toBe(true);
    await emit('diarization-progress', {
      meeting_id: 'meeting-a', kind: 'naming', state: 'queued', stage: null, percent: 0, message: 'Waiting to start…',
    });
    expect(state.autoNamingPending).toBe(false);
  });

  test('a naming error or a refused automatic guess ends the pending state', async () => {
    await act(async () => { await state.guessNames(true, false); });
    await emit('diarization-error', {
      meeting_id: 'meeting-a', kind: 'naming', error: 'No summary model is configured', automatic: true, cancelled: false,
    });
    expect(state.autoNamingPending).toBe(false);
    expect(failure).not.toHaveBeenCalled();

    guessRefusal = 'Speaker identification is already running for this meeting';
    let thrown: unknown = null;
    await act(async () => { await state.guessNames(true, false).catch((error: unknown) => { thrown = error; }); });
    expect(thrown).toBe(guessRefusal);
    expect(state.autoNamingPending).toBe(false);
  });
});
