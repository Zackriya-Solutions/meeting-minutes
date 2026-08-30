/**
 * Recording Service
 *
 * Handles all recording lifecycle Tauri backend calls and events.
 * Pure 1-to-1 wrapper - no error handling changes, exact same behavior as direct invoke/listen calls.
 */

import { invoke } from '@tauri-apps/api/core';
import { listen, UnlistenFn } from '@tauri-apps/api/event';

export interface RecordingState {
  is_recording: boolean;
  is_paused: boolean;
  is_active: boolean;
  recording_duration: number | null;
  active_duration: number | null;
}

export interface RecordingStoppedPayload {
  message: string;
  folder_path?: string;
  meeting_name?: string;
}

// Bound the start invoke: > ~40s Bluetooth mic cold-start and ~90s worst-case
// Windows device enumeration, but still finite so a hung native start settles
// the UI into ERROR instead of an eternal STARTING spinner.
const START_TIMEOUT_MS = 120000;

// ponytail: on timeout we only reject — no auto stop_recording. Calling stop
// against a stuck start could itself block; the ERROR state lets the user
// retry and the backend engine-lifecycle lock serializes that retry.
function withStartTimeout<T>(promise: Promise<T>): Promise<T> {
  return new Promise<T>((resolve, reject) => {
    const timer = setTimeout(
      () => reject(new Error('Recording start timed out after 120s')),
      START_TIMEOUT_MS
    );
    promise.then(
      (value) => { clearTimeout(timer); resolve(value); },
      (err) => { clearTimeout(timer); reject(err); }
    );
  });
}

/**
 * Recording Service
 * Singleton service for managing recording lifecycle operations
 */
export class RecordingService {
  /**
   * Check if recording is currently active
   * @returns Promise<boolean>
   */
  async isRecording(): Promise<boolean> {
    return invoke<boolean>('is_recording');
  }

  /**
   * Get comprehensive recording state (includes durations)
   * @returns Promise with full recording state
   */
  async getRecordingState(): Promise<RecordingState> {
    return invoke<RecordingState>('get_recording_state');
  }

  /**
   * Get current meeting name
   * @returns Promise<string | null>
   */
  async getRecordingMeetingName(): Promise<string | null> {
    return invoke<string | null>('get_recording_meeting_name');
  }

  /**
   * Start recording (no device configuration)
   * @returns Promise<void>
   */
  async startRecording(): Promise<void> {
    return withStartTimeout(invoke('start_recording'));
  }

  /**
   * Start recording with device configuration and meeting name
   * @param micDeviceName - Microphone device name (null for default)
   * @param systemDeviceName - System audio device name (null for none)
   * @param meetingName - Meeting name/title
   * @returns Promise<void>
   */
  async startRecordingWithDevices(
    micDeviceName: string | null,
    systemDeviceName: string | null,
    meetingName: string
  ): Promise<void> {
    return withStartTimeout(invoke('start_recording_with_devices_and_meeting', {
      micDeviceName,
      systemDeviceName,
      meetingName
    }));
  }

  /**
   * Stop recording and save to file
   * @param savePath - Path to save audio file
   * @returns Promise<void>
   */
  async stopRecording(savePath: string): Promise<void> {
    return invoke('stop_recording', {
      args: { save_path: savePath }
    });
  }

  /**
   * Pause active recording
   * @returns Promise<void>
   */
  async pauseRecording(): Promise<void> {
    return invoke('pause_recording');
  }

  /**
   * Resume paused recording
   * @returns Promise<void>
   */
  async resumeRecording(): Promise<void> {
    return invoke('resume_recording');
  }

  // Event Listeners

  /**
   * Listen for recording-started event
   * @param callback - Function to call when recording starts
   * @returns Promise that resolves to unlisten function
   */
  async onRecordingStarted(callback: () => void): Promise<UnlistenFn> {
    return listen('recording-started', callback);
  }

  /**
   * Listen for recording-starting event (fires when start begins)
   * @param callback - Function to call when recording start begins
   * @returns Promise that resolves to unlisten function
   */
  async onRecordingStarting(callback: () => void): Promise<UnlistenFn> {
    return listen('recording-starting', callback);
  }

  /**
   * Listen for recording-stopped event (with metadata)
   * @param callback - Function to call when recording stops
   * @returns Promise that resolves to unlisten function
   */
  async onRecordingStopped(callback: (payload: RecordingStoppedPayload) => void): Promise<UnlistenFn> {
    return listen<RecordingStoppedPayload>('recording-stopped', (event) => {
      callback(event.payload);
    });
  }

  /**
   * Listen for recording-paused event
   * @param callback - Function to call when recording is paused
   * @returns Promise that resolves to unlisten function
   */
  async onRecordingPaused(callback: () => void): Promise<UnlistenFn> {
    return listen('recording-paused', callback);
  }

  /**
   * Listen for recording-resumed event
   * @param callback - Function to call when recording resumes
   * @returns Promise that resolves to unlisten function
   */
  async onRecordingResumed(callback: () => void): Promise<UnlistenFn> {
    return listen('recording-resumed', callback);
  }

  /**
   * Listen for chunk-drop-warning event (audio buffer overflow)
   * @param callback - Function to call when chunks are dropped
   * @returns Promise that resolves to unlisten function
   */
  async onChunkDropWarning(callback: (warning: string) => void): Promise<UnlistenFn> {
    return listen<string>('chunk-drop-warning', (event) => {
      callback(event.payload);
    });
  }

  /**
   * Listen for speech-detected event (VAD)
   * @param callback - Function to call when speech is detected
   * @returns Promise that resolves to unlisten function
   */
  async onSpeechDetected(callback: () => void): Promise<UnlistenFn> {
    return listen('speech-detected', callback);
  }

  /**
   * Listen for mic-device-switched event (mid-recording mic hot-swap succeeded)
   * @param callback - Function to call when the mic is switched to a new device
   * @returns Promise that resolves to unlisten function
   */
  async onMicDeviceSwitched(callback: (payload: { device_name: string }) => void): Promise<UnlistenFn> {
    return listen<{ device_name: string }>('mic-device-switched', (event) => {
      callback(event.payload);
    });
  }

  /**
   * Listen for mic-swap-failed event (mid-recording mic hot-swap failed)
   * @param callback - Function to call when the mic swap fails
   * @returns Promise that resolves to unlisten function
   */
  async onMicSwapFailed(callback: (payload: { error: string; device_name: string }) => void): Promise<UnlistenFn> {
    return listen<{ error: string; device_name: string }>('mic-swap-failed', (event) => {
      callback(event.payload);
    });
  }
}

// Export singleton instance
export const recordingService = new RecordingService();
