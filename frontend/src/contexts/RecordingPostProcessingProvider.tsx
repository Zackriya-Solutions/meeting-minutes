'use client';

import React, { useCallback, useEffect } from 'react';
import { listen } from '@tauri-apps/api/event';
import { useRecordingStop } from '@/hooks/useRecordingStop';

export const REQUEST_RECORDING_STOP_EVENT = 'memento:request-recording-stop';

/**
 * RecordingPostProcessingProvider
 *
 * This provider handles post-processing when recording stops from any source:
 * - Tray menu stop
 * - Global keyboard shortcut
 * - Overlay stop button
 * - Main UI stop button
 *
 * It listens for the 'recording-stop-complete' event from Rust backend
 * and triggers the full post-processing flow (save to database, navigate, analytics)
 * regardless of which page the user is currently on.
 */
export function RecordingPostProcessingProvider({ children }: { children: React.ReactNode }) {
  // No-op functions since the global RecordingStateContext already handles state updates
  // These are only needed for the hook's local component state management
  const setIsRecording = () => { };
  const setIsRecordingDisabled = () => { };

  const {
    handleRecordingStop,
    setIsStopping,
  } = useRecordingStop(setIsRecording, setIsRecordingDisabled);

  // Flip to STOPPING before the async finalize so the pill shows its saving state on
  // the first frame, matching what the recording screen's Finish button does.
  const stopRecording = useCallback(() => {
    setIsStopping(true);
    void handleRecordingStop(true);
  }, [handleRecordingStop, setIsStopping]);

  useEffect(() => {
    const handleStopRequest = () => stopRecording();
    window.addEventListener(REQUEST_RECORDING_STOP_EVENT, handleStopRequest);
    return () => {
      window.removeEventListener(REQUEST_RECORDING_STOP_EVENT, handleStopRequest);
    };
  }, [stopRecording]);

  useEffect(() => {
    let unlistenFn: (() => void) | undefined;

    const setupListener = async () => {
      try {
        // Listen for recording-stop-complete event from Rust
        unlistenFn = await listen<boolean>('recording-stop-complete', (event) => {
          console.log('[RecordingPostProcessing] Received recording-stop-complete event:', event.payload);

          // Call the post-processing handler
          // event.payload is the callApi boolean (true for normal stops)
          handleRecordingStop(event.payload);
        });

        console.log('[RecordingPostProcessing] Event listener set up successfully');
      } catch (error) {
        console.error('[RecordingPostProcessing] Failed to set up event listener:', error);
      }
    };

    setupListener();

    return () => {
      if (unlistenFn) {
        console.log('[RecordingPostProcessing] Cleaning up event listener');
        unlistenFn();
      }
    };
  }, [handleRecordingStop]);

  return <>{children}</>;
}
