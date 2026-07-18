import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';

// VALUEOS: drives the controller's transcript-update listener directly to assert the streaming
// live-view wiring end-to-end through the hook: an INTERIM (is_partial) update replaces the single
// preview buffer, a FINAL update commits the line and clears the preview. Also asserts that a
// system-sourced segment is labelled "Other" and a mic-sourced one "Me".
const state = vi.hoisted(() => ({
  handler: null as null | ((e: { payload: unknown }) => void),
  clearTranscripts: vi.fn(),
  flushBuffer: vi.fn(),
  startRec: vi.fn(async () => {}),
  invoke: vi.fn(async (cmd: string) => (cmd === 'parakeet_has_available_models' ? true : undefined)),
}));

// /core, /path and /event all alias to this stub (see vitest.config.ts).
vi.mock('./tauri-stub', () => ({
  invoke: (...a: unknown[]) => state.invoke(...a),
  appDataDir: async () => '/appdata',
  join: async (...p: string[]) => p.join('/'),
  listen: async (_event: string, cb: (e: { payload: unknown }) => void) => {
    state.handler = cb;
    return () => {
      state.handler = null;
    };
  },
}));
vi.mock('@/contexts/TranscriptContext', () => ({
  useTranscripts: () => ({
    transcripts: [],
    transcriptsRef: { current: [] },
    clearTranscripts: state.clearTranscripts,
    flushBuffer: state.flushBuffer,
  }),
}));
vi.mock('@/contexts/RecordingStateContext', () => ({
  useRecordingState: () => ({ isRecording: true, status: 'recording' }),
}));
vi.mock('@/contexts/ConfigContext', () => ({
  useConfig: () => ({ selectedDevices: { micDevice: null, systemDevice: null } }),
}));
vi.mock('@/services/recordingService', () => ({
  recordingService: {
    startRecordingWithDevices: (...a: unknown[]) => state.startRec(...a),
    stopRecording: async () => {},
  },
}));

import { useRecordingController } from '@/valueos/capture/useRecordingController';

function emit(payload: Record<string, unknown>) {
  act(() => {
    state.handler?.({ payload });
  });
}

beforeEach(() => {
  state.handler = null;
});

describe('streaming interim live view', () => {
  it('shows preview words that REPLACE (not append), then commits on the final', async () => {
    const { result } = renderHook(() => useRecordingController());
    await act(async () => {
      await result.current.start('Discovery Call');
    });
    expect(state.handler).toBeTypeOf('function');

    // Interim hypotheses: each replaces the preview; nothing is committed yet.
    emit({ text: 'Where you', is_partial: true, source: 'Other' });
    expect(result.current.partialText).toBe('Where you');
    expect(result.current.confirmedText).toBe('');

    emit({ text: 'Where you are joining', is_partial: true, source: 'Other' });
    expect(result.current.partialText).toBe('Where you are joining'); // replaced, not appended
    expect(result.current.confirmedText).toBe('');

    // The final commits the line and CLEARS the preview.
    emit({ text: 'Where you are joining us from.', is_partial: false, sequence_id: 0, source: 'Other' });
    expect(result.current.partialText).toBe('');
    expect(result.current.confirmedText).toContain('Where you are joining us from.');
    // System-audio speech is labelled Other (the reported bug: it must NOT be "Me").
    expect(result.current.confirmedText).toContain('Other:');
  });

  it('labels a mic-sourced final as Me', async () => {
    const { result } = renderHook(() => useRecordingController());
    await act(async () => {
      await result.current.start('Call');
    });
    emit({ text: 'Thanks for joining.', is_partial: false, sequence_id: 0, source: 'Me' });
    expect(result.current.confirmedText).toContain('Me: Thanks for joining.');
  });
});
