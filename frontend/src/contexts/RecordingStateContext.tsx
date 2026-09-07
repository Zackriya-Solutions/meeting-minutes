'use client';

import React, { createContext, useContext, useState, useEffect, useRef, useCallback, useMemo } from 'react';
import { recordingService } from '@/services/recordingService';
import { toast } from 'sonner';

/**
 * Recording state synchronized with backend
 * This context provides a single source of truth for recording state
 * that automatically syncs with the Rust backend, solving:
 * 1. Page refresh desync (backend recording but UI shows stopped)
 * 2. Pause state visibility across components
 * 3. Comprehensive state for future features (reconnection, etc.)
 */

// Recording lifecycle status enum
export enum RecordingStatus {
  IDLE = 'idle',                          // Not recording
  STARTING = 'starting',                  // Initiating recording
  RECORDING = 'recording',                // Active recording
  STOPPING = 'stopping',                  // Stop initiated, waiting for backend
  PROCESSING_TRANSCRIPTS = 'processing',  // Transcription completion wait
  SAVING = 'saving',                      // Saving to database
  COMPLETED = 'completed',                // Successfully saved
  ERROR = 'error'                         // Error occurred
}

interface RecordingState {
  isRecording: boolean;           // Is a recording session active
  isPaused: boolean;              // Is the recording paused
  isActive: boolean;              // Is actively recording (recording && !paused)
  recordingDuration: number | null;  // Total duration including pauses
  activeDuration: number | null;     // Active recording time (excluding pauses)

  // NEW: Lifecycle status
  status: RecordingStatus;
  statusMessage?: string;  // Optional message for current status
}

interface RecordingStateContextType extends RecordingState {
  // NEW: Setters for status management
  setStatus: (status: RecordingStatus, message?: string) => void;

  // Computed helpers (derived from status)
  isStopping: boolean;
  isProcessing: boolean;
  isSaving: boolean;
  isStartingRecording: boolean;
}

const RecordingStateContext = createContext<RecordingStateContextType | null>(null);

export const useRecordingState = () => {
  const context = useContext(RecordingStateContext);
  if (!context) {
    throw new Error('useRecordingState must be used within a RecordingStateProvider');
  }
  return context;
};

export function RecordingStateProvider({ children }: { children: React.ReactNode }) {
  const [state, setState] = useState<RecordingState>({
    isRecording: false,
    isPaused: false,
    isActive: false,
    recordingDuration: null,
    activeDuration: null,
    status: RecordingStatus.IDLE,  // NEW: Initialize with IDLE status
    statusMessage: undefined,       // NEW: No message initially
  });

  const pollingIntervalRef = useRef<NodeJS.Timeout | null>(null);

  // NEW: Status setter with logging
  const setStatus = useCallback((status: RecordingStatus, message?: string) => {
    console.log(`[RecordingState] Status: ${state.status} → ${status}`, message || '');

    setState(prev => ({
      ...prev,
      status,
      statusMessage: message,
    }));
  }, [state.status, state.isRecording, state.isPaused]);

  /**
   * Sync recording state with backend
   * Called on mount (fixes refresh desync) and periodically while recording
   */
  const syncWithBackend = async () => {
    try {
      const backendState = await recordingService.getRecordingState();

      setState(prev => ({
        ...prev,
        isRecording: backendState.is_recording,
        isPaused: backendState.is_paused,
        isActive: backendState.is_active,
        recordingDuration: backendState.recording_duration,
        activeDuration: backendState.active_duration,
      }));

      console.log('[RecordingStateContext] Synced with backend:', backendState);
    } catch (error) {
      console.error('[RecordingStateContext] Failed to sync with backend:', error);
      // Don't update state on error - keep current state
    }
  };

  /**
   * Start polling backend state (called when recording starts)
   */
  const startPolling = () => {
    if (pollingIntervalRef.current) {
      clearInterval(pollingIntervalRef.current);
    }

    console.log('[RecordingStateContext] Starting state polling (500ms interval)');
    pollingIntervalRef.current = setInterval(syncWithBackend, 500);
  };

  /**
   * Stop polling backend state (called when recording stops)
   */
  const stopPolling = () => {
    if (pollingIntervalRef.current) {
      console.log('[RecordingStateContext] Stopping state polling');
      clearInterval(pollingIntervalRef.current);
      pollingIntervalRef.current = null;
    }
  };

  /**
   * Set up event listeners for backend state changes
   */
  useEffect(() => {
    console.log('[RecordingStateContext] Setting up event listeners');
    const unsubscribers: (() => void)[] = [];

    const setupListeners = async () => {
      try {
        // Recording started
        const unlistenStarted = await recordingService.onRecordingStarted(() => {
          console.log('[RecordingStateContext] Recording started event');
          setState(prev => ({
            ...prev,
            isRecording: true,
            isPaused: false,
            isActive: true,
            status: RecordingStatus.RECORDING,  // NEW: Set status to RECORDING
          }));
          startPolling();
        });
        unsubscribers.push(unlistenStarted);

        // Recording starting (startup in progress)
        const unlistenStarting = await recordingService.onRecordingStarting(() => {
          console.log('[RecordingStateContext] Recording starting event');
          setState(prev => prev.status === RecordingStatus.RECORDING
            ? prev
            : { ...prev, status: RecordingStatus.STARTING, statusMessage: 'Starting recording...' });
        });
        unsubscribers.push(unlistenStarting);

        // Recording stopped
        const unlistenStopped = await recordingService.onRecordingStopped((payload) => {
          console.log('[RecordingStateContext] Recording stopped event:', payload);
          setState(prev => {
            // Set status to STOPPING if not already in stop flow
            // This ensures smooth UI transition for tray/keyboard stops
            const newStatus = [
              RecordingStatus.STOPPING,
              RecordingStatus.PROCESSING_TRANSCRIPTS,
              RecordingStatus.SAVING
            ].includes(prev.status)
              ? prev.status  // Already in stop flow
              : RecordingStatus.STOPPING;  // New stop, transition smoothly

            return {
              ...prev,
              status: newStatus,
              statusMessage: newStatus === RecordingStatus.STOPPING ? 'Stopping recording...' : prev.statusMessage,
              isRecording: false,
              isPaused: false,
              isActive: false,
              recordingDuration: null,
              activeDuration: null,
            };
          });
          stopPolling();
        });
        unsubscribers.push(unlistenStopped);

        // Recording paused
        const unlistenPaused = await recordingService.onRecordingPaused(() => {
          console.log('[RecordingStateContext] Recording paused event');
          setState(prev => ({
            ...prev,
            isPaused: true,
            isActive: false,
          }));
        });
        unsubscribers.push(unlistenPaused);

        // Recording resumed
        const unlistenResumed = await recordingService.onRecordingResumed(() => {
          console.log('[RecordingStateContext] Recording resumed event');
          setState(prev => ({
            ...prev,
            isPaused: false,
            isActive: true,
          }));
        });
        unsubscribers.push(unlistenResumed);

        console.log('[RecordingStateContext] Event listeners set up successfully');
      } catch (error) {
        console.error('[RecordingStateContext] Failed to set up event listeners:', error);
      }
    };

    setupListeners();

    return () => {
      console.log('[RecordingStateContext] Cleaning up event listeners');
      unsubscribers.forEach(unsub => unsub());
      stopPolling();
    };
  }, []);

  /**
   * Global mic hot-swap UI feedback.
   *
   * The Rust backend emits `mic-device-switched` whenever the recording mic
   * changes without the user picking it — on a successful mid-recording
   * disconnect fallback, and when a selected mic isn't available at start and
   * the backend falls back to the default — and `mic-swap-failed` when
   * mid-recording recovery fails. Nothing else in the app listens for these,
   * so without this effect the switch is silent (or a dead-mic recording).
   * Mounting the listener here surfaces a toast regardless of which page the
   * user is on. Pure UI feedback — no state mutation, no persisted-preference
   * writes.
   */
  // Ref tracks latest isRecording for the mount-once listener below — the
  // effect has [] deps so it can't read state directly (it would capture the
  // initial value). Kept current by this small sync effect.
  const isRecordingRef = useRef(false);
  useEffect(() => {
    isRecordingRef.current = state.isRecording;
  }, [state.isRecording]);

  useEffect(() => {
    // `cancelled` guard prevents leaking a listener when StrictMode/HMR runs
    // cleanup before the async listen(...) registration resolves.
    let cancelled = false;
    let unlistenSwitched: (() => void) | undefined;
    let unlistenFailed: (() => void) | undefined;
    let unlistenMicUnavailable: (() => void) | undefined;
    let unlistenExhausted: (() => void) | undefined;

    const setup = async () => {
      try {
        const fnSwitched = await recordingService.onMicDeviceSwitched(({ device_name }) => {
          console.log('[RecordingStateContext] mic-device-switched →', device_name);
          // Fires for both cases: a mic that disconnects mid-recording, and a
          // selected mic that wasn't available at start (backend fell back to
          // the default). Copy is worded to be accurate for both.
          toast.info(
            `Microphone switched to ${device_name} for this meeting.`,
            { duration: 6000 }
          );
        });
        if (cancelled) { fnSwitched(); return; }
        unlistenSwitched = fnSwitched;

        const fnFailed = await recordingService.onMicSwapFailed(({ error, device_name }) => {
          console.error('[RecordingStateContext] mic-swap-failed →', device_name, error);
          // Only alarm the user if recording is still active. A Stop clicked
          // during a hot-swap race fails with "Recording manager not
          // available" — expected, not an error worth a toast.
          if (isRecordingRef.current) {
            toast.error(
              `Microphone fallback failed for ${device_name}: ${error}`,
              { duration: 8000 }
            );
          }
        });
        if (cancelled) { fnFailed(); return; }
        unlistenFailed = fnFailed;

        const fnUnavailable = await recordingService.onMicUnavailable(() => {
          console.log('[RecordingStateContext] mic-unavailable');
          // Fires at recording start, before isRecording flips true, so this
          // is intentionally NOT gated by isRecordingRef.
          toast.error(
            'No microphone available — recording system audio only.',
            { duration: 8000 }
          );
        });
        if (cancelled) { fnUnavailable(); return; }
        unlistenMicUnavailable = fnUnavailable;

        const fnExhausted = await recordingService.onMicRecoveryExhausted(({ device_name }) => {
          console.error('[RecordingStateContext] mic-recovery-exhausted →', device_name);
          // Fires mid-recording only — gate on the recording ref like
          // mic-swap-failed so a stale event after Stop doesn't alarm the user.
          if (isRecordingRef.current) {
            toast.error(
              `Microphone '${device_name}' could not be recovered — recording continues without a microphone. Stop and restart to fix.`,
              { duration: 10000 }
            );
          }
        });
        if (cancelled) { fnExhausted(); return; }
        unlistenExhausted = fnExhausted;
      } catch (e) {
        console.error('[RecordingStateContext] Failed to set up hot-swap listeners:', e);
      }
    };

    setup();

    return () => {
      cancelled = true;
      unlistenSwitched?.();
      unlistenFailed?.();
      unlistenMicUnavailable?.();
      unlistenExhausted?.();
    };
  }, []);

  /**
   * Initial sync on mount - CRITICAL for fixing refresh desync bug
   * If backend is recording but UI state is false, this will correct it
   */
  useEffect(() => {
    console.log('[RecordingStateContext] Initial mount - syncing with backend');
    syncWithBackend();
  }, []);

  // NEW: Computed helpers from status
  const contextValue = useMemo(() => ({
    ...state,
    setStatus,
    isStopping: state.status === RecordingStatus.STOPPING,
    isProcessing: state.status === RecordingStatus.PROCESSING_TRANSCRIPTS,
    isSaving: state.status === RecordingStatus.SAVING,
    isStartingRecording: state.status === RecordingStatus.STARTING,
  }), [state, setStatus]);

  return (
    <RecordingStateContext.Provider value={contextValue}>
      {children}
    </RecordingStateContext.Provider>
  );
}
