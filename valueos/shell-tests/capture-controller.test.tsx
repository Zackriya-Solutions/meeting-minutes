import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';

// Mock the native + upstream deps the controller adapts over.
const state = vi.hoisted(() => ({
  segments: [] as { text: string }[],
  hasModels: true,
  clearTranscripts: vi.fn(),
  flushBuffer: vi.fn(),
  startRec: vi.fn(async () => {}),
  stopRec: vi.fn(async () => {}),
  invoke: vi.fn(),
}));

// The vitest config aliases BOTH @tauri-apps/api/core and /path to this single stub file,
// so mock the shared target once with every export the controller uses.
vi.mock('./tauri-stub', () => ({
  invoke: (...a: unknown[]) => state.invoke(...a),
  appDataDir: async () => '/appdata',
  join: async (...p: string[]) => p.join('/'),
  listen: async () => () => {}, // no native events fire in this unit test
}));
vi.mock('@/contexts/TranscriptContext', () => ({
  useTranscripts: () => ({
    transcripts: state.segments,
    transcriptsRef: { current: state.segments },
    clearTranscripts: state.clearTranscripts,
    flushBuffer: state.flushBuffer,
  }),
}));
vi.mock('@/contexts/RecordingStateContext', () => ({
  useRecordingState: () => ({ isRecording: false, status: 'idle' }),
}));
vi.mock('@/contexts/ConfigContext', () => ({
  useConfig: () => ({ selectedDevices: { micDevice: null, systemDevice: null } }),
}));
vi.mock('@/services/recordingService', () => ({
  recordingService: {
    startRecordingWithDevices: (...a: unknown[]) => state.startRec(...a),
    stopRecording: (...a: unknown[]) => state.stopRec(...a),
  },
}));

import { useRecordingController } from '@/valueos/capture/useRecordingController';

beforeEach(() => {
  state.segments = [];
  state.hasModels = true;
  state.clearTranscripts.mockClear();
  state.flushBuffer.mockClear();
  state.startRec.mockClear();
  state.stopRec.mockClear();
  state.invoke.mockReset();
  state.invoke.mockImplementation(async (cmd: string) => {
    if (cmd === 'parakeet_has_available_models') return state.hasModels;
    return undefined;
  });
});

describe('useRecordingController', () => {
  it('initializes the transcription engine BEFORE recording (fixes empty transcript)', async () => {
    const { result } = renderHook(() => useRecordingController());
    await act(async () => {
      await result.current.start('Acme — meeting');
    });
    expect(state.invoke).toHaveBeenCalledWith('parakeet_init');
    expect(state.invoke).toHaveBeenCalledWith('parakeet_has_available_models');
    expect(state.startRec).toHaveBeenCalledWith(null, null, 'Acme — meeting');
    expect(state.clearTranscripts).toHaveBeenCalled();
  });

  it('refuses to start (and does not record) when no model is available', async () => {
    state.hasModels = false;
    const { result } = renderHook(() => useRecordingController());
    await expect(result.current.start('m')).rejects.toThrow(/model/i);
    expect(state.startRec).not.toHaveBeenCalled();
  });

  it('stop() flushes and returns the enriched (timestamped + labelled) transcript', async () => {
    state.segments = [{ text: 'Hello Ada.' }, { text: 'Discussed pricing.' }, { text: '  ' }];
    const { result } = renderHook(() => useRecordingController());
    let text = '';
    await act(async () => {
      text = await result.current.stop();
    });
    expect(state.stopRec).toHaveBeenCalled();
    expect(state.flushBuffer).toHaveBeenCalled();
    // Enriched format: "[HH:MM:SS] Label: text". No speaker source in this stub → fallback label;
    // no times → 00:00:00. Blanks dropped.
    expect(text).toContain('[00:00:00] Speaker: Hello Ada.');
    expect(text).toContain('[00:00:00] Speaker: Discussed pricing.');
    expect(text).not.toContain('Speaker: \n'); // the blank segment produced no line
  });

  it('exposes live transcriptText from the current segments', () => {
    state.segments = [{ text: 'live one' }, { text: 'live two' }];
    const { result } = renderHook(() => useRecordingController());
    expect(result.current.transcriptText).toBe('live one\nlive two');
  });
});
