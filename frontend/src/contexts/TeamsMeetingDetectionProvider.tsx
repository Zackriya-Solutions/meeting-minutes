'use client';

import React, { useEffect, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { appDataDir } from '@tauri-apps/api/path';
import { toast } from 'sonner';
import { useConfig } from '@/contexts/ConfigContext';
import { useRecordingState } from '@/contexts/RecordingStateContext';
import Analytics from '@/lib/analytics';

interface TeamsCallLikelyStartedPayload {
  type: string;
  confidence: number;
  teams_running: boolean;
  teams_audio_active: boolean;
  poll_interval_seconds: number;
}

interface TeamsCallLikelyEndedPayload {
  type: string;
  confidence: number;
  teams_running: boolean;
  teams_audio_active: boolean;
  inactive_polls: number;
}

const START_EVENT = 'teams-call-likely-started';
const END_EVENT = 'teams-call-likely-ended';
const START_COOLDOWN_KEY = 'teams_meeting_detection_start_dismissed_until';
const STOP_COOLDOWN_KEY = 'teams_meeting_detection_stop_dismissed_until';

export function TeamsMeetingDetectionProvider({ children }: { children: React.ReactNode }) {
  const recordingState = useRecordingState();
  const {
    meetingDetectionSettings,
    loadPreferences,
    updateMeetingDetectionSettings,
  } = useConfig();

  const recordingStateRef = useRef(recordingState);
  const settingsRef = useRef(meetingDetectionSettings);

  useEffect(() => {
    recordingStateRef.current = recordingState;
  }, [recordingState]);

  useEffect(() => {
    settingsRef.current = meetingDetectionSettings;
  }, [meetingDetectionSettings]);

  useEffect(() => {
    loadPreferences().catch(error => {
      console.error('[TeamsMeetingDetection] Failed to load preferences:', error);
    });
  }, [loadPreferences]);

  useEffect(() => {
    let cleanupFns: Array<() => void> = [];
    let cleanedUp = false;

    const setupListeners = async () => {
      const unlistenStart = await listen<TeamsCallLikelyStartedPayload>(START_EVENT, (event) => {
        handleStartPrompt(event.payload);
      });
      if (cleanedUp) {
        unlistenStart();
        return;
      }
      cleanupFns.push(unlistenStart);

      const unlistenEnd = await listen<TeamsCallLikelyEndedPayload>(END_EVENT, (event) => {
        handleStopPrompt(event.payload);
      });
      if (cleanedUp) {
        unlistenEnd();
        cleanupFns.forEach(cleanup => cleanup());
        return;
      }
      cleanupFns.push(unlistenEnd);
    };

    setupListeners().catch(error => {
      console.error('[TeamsMeetingDetection] Failed to set up listeners:', error);
    });

    return () => {
      cleanedUp = true;
      cleanupFns.forEach(cleanup => cleanup());
      cleanupFns = [];
    };
  }, []);

  const getCooldownUntil = (key: string) => {
    if (typeof window === 'undefined') return 0;
    const value = window.localStorage.getItem(key);
    return value ? Number(value) || 0 : 0;
  };

  const setCooldown = (key: string) => {
    const settings = settingsRef.current;
    const cooldownMinutes = settings?.teams_prompt_cooldown_minutes ?? 30;
    window.localStorage.setItem(
      key,
      String(Date.now() + cooldownMinutes * 60 * 1000)
    );
  };

  const isInCooldown = (key: string) => Date.now() < getCooldownUntil(key);

  const disablePrompt = async (type: 'start' | 'stop') => {
    const settings = settingsRef.current;
    if (!settings) return;

    await updateMeetingDetectionSettings({
      ...settings,
      teams_prompt_start: type === 'start' ? false : settings.teams_prompt_start,
      teams_prompt_stop: type === 'stop' ? false : settings.teams_prompt_stop,
    });
  };

  const handleStartPrompt = (payload: TeamsCallLikelyStartedPayload) => {
    const settings = settingsRef.current;
    const currentRecordingState = recordingStateRef.current;

    if (!settings?.meeting_detection_enabled || !settings.teams_prompt_start) return;
    if (currentRecordingState.isRecording || currentRecordingState.isStopping) return;
    if (isInCooldown(START_COOLDOWN_KEY)) return;

    let toastId: string | number;
    toastId = toast.info('Teams appears to be in a call', {
      description: (
        <div className="space-y-3 min-w-[280px]">
          <p className="text-sm text-gray-700">Start a Meetily recording?</p>
          <div className="flex flex-wrap gap-2">
            <button
              onClick={() => {
                toast.dismiss(toastId);
                Analytics.trackButtonClick('teams_detection_start_recording', 'teams_detection_prompt');
                sessionStorage.setItem('autoStartRecording', 'true');
                window.location.assign('/');
              }}
              className="px-3 py-1.5 text-xs font-medium rounded bg-gray-900 text-white hover:bg-gray-800"
            >
              Start Recording
            </button>
            <button
              onClick={() => {
                toast.dismiss(toastId);
                setCooldown(START_COOLDOWN_KEY);
                Analytics.trackButtonClick('teams_detection_start_dismissed', 'teams_detection_prompt');
              }}
              className="px-3 py-1.5 text-xs font-medium rounded border border-gray-300 text-gray-700 hover:bg-gray-50"
            >
              Dismiss
            </button>
            <button
              onClick={async () => {
                toast.dismiss(toastId);
                await disablePrompt('start');
                Analytics.trackButtonClick('teams_detection_start_disabled', 'teams_detection_prompt');
              }}
              className="px-3 py-1.5 text-xs font-medium rounded border border-gray-300 text-gray-700 hover:bg-gray-50"
            >
              Do not ask again
            </button>
          </div>
        </div>
      ),
      duration: 15000,
      position: 'bottom-right',
    });

    console.log('[TeamsMeetingDetection] Start prompt payload:', payload);
  };

  const handleStopPrompt = (payload: TeamsCallLikelyEndedPayload) => {
    const settings = settingsRef.current;
    const currentRecordingState = recordingStateRef.current;

    if (!settings?.meeting_detection_enabled || !settings.teams_prompt_stop) return;
    if (!currentRecordingState.isRecording || currentRecordingState.isStopping) return;
    if (isInCooldown(STOP_COOLDOWN_KEY)) return;

    let toastId: string | number;
    toastId = toast.info('Teams audio appears inactive', {
      description: (
        <div className="space-y-3 min-w-[280px]">
          <p className="text-sm text-gray-700">Stop the current Meetily recording?</p>
          <div className="flex flex-wrap gap-2">
            <button
              onClick={async () => {
                toast.dismiss(toastId);
                await stopRecordingFromPrompt();
                Analytics.trackButtonClick('teams_detection_stop_recording', 'teams_detection_prompt');
              }}
              className="px-3 py-1.5 text-xs font-medium rounded bg-gray-900 text-white hover:bg-gray-800"
            >
              Stop Recording
            </button>
            <button
              onClick={() => {
                toast.dismiss(toastId);
                setCooldown(STOP_COOLDOWN_KEY);
                Analytics.trackButtonClick('teams_detection_stop_dismissed', 'teams_detection_prompt');
              }}
              className="px-3 py-1.5 text-xs font-medium rounded border border-gray-300 text-gray-700 hover:bg-gray-50"
            >
              Dismiss
            </button>
            <button
              onClick={async () => {
                toast.dismiss(toastId);
                await disablePrompt('stop');
                Analytics.trackButtonClick('teams_detection_stop_disabled', 'teams_detection_prompt');
              }}
              className="px-3 py-1.5 text-xs font-medium rounded border border-gray-300 text-gray-700 hover:bg-gray-50"
            >
              Do not ask again
            </button>
          </div>
        </div>
      ),
      duration: 15000,
      position: 'bottom-right',
    });

    console.log('[TeamsMeetingDetection] Stop prompt payload:', payload);
  };

  const stopRecordingFromPrompt = async () => {
    try {
      const dataDir = await appDataDir();
      const timestamp = new Date().toISOString().replace(/[:.]/g, '-');
      const savePath = `${dataDir}/recording-${timestamp}.wav`;

      await invoke('stop_recording', {
        args: {
          save_path: savePath,
        },
      });

      const stopHandler = (window as typeof window & {
        handleRecordingStop?: (callApi?: boolean) => void;
      }).handleRecordingStop;

      if (stopHandler) {
        stopHandler(true);
      } else {
        window.dispatchEvent(new CustomEvent('teams-detection-recording-stopped'));
      }
    } catch (error) {
      console.error('[TeamsMeetingDetection] Failed to stop recording:', error);
      toast.error('Failed to stop recording', {
        description: error instanceof Error ? error.message : String(error),
      });
    }
  };

  return <>{children}</>;
}
