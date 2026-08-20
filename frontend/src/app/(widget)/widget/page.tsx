'use client';

import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { Pause, Play, Square } from 'lucide-react';
import { recordingService, RecordingState } from '@/services/recordingService';

// Backstop poll in case an event is missed (e.g. widget shown mid-recording);
// recording-started/paused/resumed/stopped listeners handle the fast path.
const POLL_INTERVAL_MS = 1000;

const INITIAL_STATE: RecordingState = {
  is_recording: false,
  is_paused: false,
  is_active: false,
  recording_duration: null,
  active_duration: null,
};

function formatElapsed(seconds: number): string {
  const total = Math.max(0, Math.floor(seconds));
  const mins = Math.floor(total / 60);
  const secs = total % 60;
  return `${mins.toString().padStart(2, '0')}:${secs.toString().padStart(2, '0')}`;
}

export default function WidgetPage() {
  const [state, setState] = useState<RecordingState>(INITIAL_STATE);
  const [isBusy, setIsBusy] = useState(false);
  const mountedRef = useRef(true);

  const refreshState = useCallback(async () => {
    try {
      const next = await recordingService.getRecordingState();
      if (mountedRef.current) {
        setState(next);
      }
    } catch (error) {
      console.error('[Widget] Failed to fetch recording state:', error);
    }
  }, []);

  useEffect(() => {
    mountedRef.current = true;
    refreshState();

    const interval = setInterval(refreshState, POLL_INTERVAL_MS);
    const unlistenPromises = [
      recordingService.onRecordingStarted(refreshState),
      recordingService.onRecordingPaused(refreshState),
      recordingService.onRecordingResumed(refreshState),
      recordingService.onRecordingStopped(refreshState),
    ];

    return () => {
      mountedRef.current = false;
      clearInterval(interval);
      unlistenPromises.forEach((p) => p.then((unlisten) => unlisten()));
    };
  }, [refreshState]);

  const handlePause = useCallback(async () => {
    setIsBusy(true);
    try {
      await recordingService.pauseRecording();
      await refreshState();
    } catch (error) {
      console.error('[Widget] Failed to pause recording:', error);
    } finally {
      setIsBusy(false);
    }
  }, [refreshState]);

  const handleResume = useCallback(async () => {
    setIsBusy(true);
    try {
      await recordingService.resumeRecording();
      await refreshState();
    } catch (error) {
      console.error('[Widget] Failed to resume recording:', error);
    } finally {
      setIsBusy(false);
    }
  }, [refreshState]);

  const handleStop = useCallback(async () => {
    setIsBusy(true);
    try {
      // Fix 2 (Gate A review): this deliberately does NOT call recordingService.stopRecording()
      // (a bare `stop_recording` invoke). That command only stops the backend recorder -- it
      // never fires the `recording-stop-complete` event that `RecordingPostProcessingProvider`
      // (mounted only in the main window's React tree) listens for to actually save and
      // finalize the meeting. `stop_recording_from_widget` mirrors tray.rs's stop handler:
      // it computes the save path itself, stops the recording, and emits that event so the
      // meeting gets saved regardless of which window initiated the stop.
      await invoke('stop_recording_from_widget');
      await refreshState();
    } catch (error) {
      console.error('[Widget] Failed to stop recording:', error);
    } finally {
      setIsBusy(false);
    }
  }, [refreshState]);

  const elapsedSeconds = state.active_duration ?? state.recording_duration ?? 0;

  return (
    <div
      data-tauri-drag-region
      className="flex h-screen w-screen select-none items-center gap-2 rounded-full border border-white/10 bg-neutral-900/90 px-3 text-white shadow-lg"
    >
      <div
        className={`h-2 w-2 flex-shrink-0 rounded-full ${
          state.is_paused ? 'bg-orange-400' : 'bg-red-500 animate-pulse'
        }`}
      />
      <span className="min-w-[42px] flex-shrink-0 font-mono text-sm tabular-nums">
        {formatElapsed(elapsedSeconds)}
      </span>
      <div className="flex flex-1 items-center justify-end gap-1">
        {state.is_paused ? (
          <button
            type="button"
            onClick={handleResume}
            disabled={isBusy || !state.is_recording}
            className="rounded-full p-1.5 transition-colors hover:bg-white/10 disabled:opacity-50"
            title="Resume recording"
          >
            <Play className="h-4 w-4" />
          </button>
        ) : (
          <button
            type="button"
            onClick={handlePause}
            disabled={isBusy || !state.is_recording}
            className="rounded-full p-1.5 transition-colors hover:bg-white/10 disabled:opacity-50"
            title="Pause recording"
          >
            <Pause className="h-4 w-4" />
          </button>
        )}
        <button
          type="button"
          onClick={handleStop}
          disabled={isBusy || !state.is_recording}
          className="rounded-full p-1.5 transition-colors hover:bg-red-500/80 disabled:opacity-50"
          title="Stop recording"
        >
          <Square className="h-4 w-4" />
        </button>
      </div>
    </div>
  );
}
