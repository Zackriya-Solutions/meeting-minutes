'use client';

import { useState, useEffect, useCallback, useRef } from 'react';
import { listen } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';

const COUNTDOWN_SECONDS = 15;
const START_POPUP_DISMISS_MS = 30000; // 30s — user needs time to decide

export interface UseTeamsDetectionReturn {
  showStartPopup: boolean;
  showEndPopup: boolean;
  countdown: number;
  teamsDetectionEnabled: boolean;
  setTeamsDetectionEnabled: (enabled: boolean) => Promise<void>;
  dismissStartPopup: () => void;
  handleStart: () => Promise<void>;
  handleStop: () => void;
  handleContinue: () => void;
}

export function useTeamsDetection(
  isRecording: boolean,
  onStart: () => Promise<void>,
  onStop: () => void
): UseTeamsDetectionReturn {
  const [showStartPopup, setShowStartPopup] = useState(false);
  const [showEndPopup, setShowEndPopup] = useState(false);
  const [countdown, setCountdown] = useState(COUNTDOWN_SECONDS);
  const [teamsDetectionEnabled, setTeamsDetectionEnabledState] = useState(false);

  const countdownRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const startDismissRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  // Keep a stable ref to isRecording to avoid stale closures in event listeners
  const isRecordingRef = useRef(isRecording);
  useEffect(() => { isRecordingRef.current = isRecording; }, [isRecording]);

  const clearCountdown = useCallback(() => {
    if (countdownRef.current) {
      clearInterval(countdownRef.current);
      countdownRef.current = null;
    }
  }, []);

  const clearStartDismiss = useCallback(() => {
    if (startDismissRef.current) {
      clearTimeout(startDismissRef.current);
      startDismissRef.current = null;
    }
  }, []);

  // Load persisted enabled state on mount
  useEffect(() => {
    invoke<boolean>('get_teams_detection_enabled')
      .then(setTeamsDetectionEnabledState)
      .catch(() => {});
  }, []);

  // Handle teams-meeting-started event — show popup only, never auto-start recording
  useEffect(() => {
    let unlisten: (() => void) | null = null;

    listen<{ app_name: string }>('teams-meeting-started', () => {
      if (isRecordingRef.current) return; // already recording, nothing to offer
      clearStartDismiss();
      setShowStartPopup(true);
      startDismissRef.current = setTimeout(() => {
        setShowStartPopup(false);
      }, START_POPUP_DISMISS_MS);
    }).then(fn => { unlisten = fn; });

    return () => { unlisten?.(); clearStartDismiss(); };
  }, [clearStartDismiss]);

  // Handle teams-meeting-ended event
  useEffect(() => {
    let unlisten: (() => void) | null = null;

    listen('teams-meeting-ended', () => {
      if (!isRecordingRef.current) return;
      clearCountdown();
      setCountdown(COUNTDOWN_SECONDS);
      setShowEndPopup(true);
    }).then(fn => { unlisten = fn; });

    return () => { unlisten?.(); };
  }, [clearCountdown]);

  // Keep a stable ref to onStop to avoid stale closure in countdown interval
  const onStopRef = useRef(onStop);
  useEffect(() => { onStopRef.current = onStop; }, [onStop]);

  // Countdown ticker when end popup is shown
  useEffect(() => {
    if (!showEndPopup) return;

    countdownRef.current = setInterval(() => {
      setCountdown(prev => {
        if (prev <= 1) {
          // Schedule side-effects outside the state updater to stay pure
          setTimeout(() => {
            clearInterval(countdownRef.current!);
            countdownRef.current = null;
            setShowEndPopup(false);
            onStopRef.current();
          }, 0);
          return 0;
        }
        return prev - 1;
      });
    }, 1000);

    return clearCountdown;
  }, [showEndPopup, clearCountdown]);

  const dismissStartPopup = useCallback(() => {
    clearStartDismiss();
    setShowStartPopup(false);
  }, [clearStartDismiss]);

  const handleStart = useCallback(async () => {
    clearStartDismiss();
    setShowStartPopup(false);
    await onStart();
  }, [clearStartDismiss, onStart]);

  const handleStop = useCallback(() => {
    clearCountdown();
    setShowEndPopup(false);
    onStop();
  }, [clearCountdown, onStop]);

  const handleContinue = useCallback(() => {
    clearCountdown();
    setShowEndPopup(false);
  }, [clearCountdown]);

  const setTeamsDetectionEnabled = useCallback(async (enabled: boolean) => {
    await invoke('set_teams_detection_enabled', { enabled });
    setTeamsDetectionEnabledState(enabled);
  }, []);

  return {
    showStartPopup,
    showEndPopup,
    countdown,
    teamsDetectionEnabled,
    setTeamsDetectionEnabled,
    dismissStartPopup,
    handleStart,
    handleStop,
    handleContinue,
  };
}
