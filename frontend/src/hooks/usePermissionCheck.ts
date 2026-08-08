import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';

export interface PermissionStatus {
  hasMicrophone: boolean;
  hasSystemAudio: boolean;
  isChecking: boolean;
  error: string | null;
}

export function usePermissionCheck() {
  const [status, setStatus] = useState<PermissionStatus>({
    hasMicrophone: false,
    hasSystemAudio: false,
    isChecking: true,
    error: null,
  });

  const checkPermissions = async () => {
    setStatus(prev => ({ ...prev, isChecking: true, error: null }));

    try {
      // Get audio devices to check for microphone and system audio availability
      const devices = await invoke<Array<{ name: string; device_type: 'Input' | 'Output' }>>('get_audio_devices');

      // Check for microphone devices (Input)
      const inputDevices = devices.filter(d => d.device_type === 'Input');
      const hasMicrophone = inputDevices.length > 0;

      // Check for system audio devices (Output)
      // On macOS, we need ScreenCaptureKit devices for system audio
      const outputDevices = devices.filter(d => d.device_type === 'Output');
      const hasSystemAudio = outputDevices.length > 0;

      console.log('Permission check:', {
        hasMicrophone,
        hasSystemAudio,
        inputDevices: inputDevices.length,
        outputDevices: outputDevices.length
      });

      setStatus({
        hasMicrophone,
        hasSystemAudio,
        isChecking: false,
        error: null,
      });

      return { hasMicrophone, hasSystemAudio };
    } catch (error) {
      console.error('Failed to check audio permissions:', error);
      setStatus({
        hasMicrophone: false,
        hasSystemAudio: false,
        isChecking: false,
        error: error instanceof Error ? error.message : 'Failed to check permissions',
      });
      return { hasMicrophone: false, hasSystemAudio: false };
    }
  };

  const requestPermissions = async () => {
    try {
      // Trigger audio permission by trying to access devices
      await invoke('get_audio_devices');

      // Recheck after triggering
      setTimeout(() => {
        checkPermissions();
      }, 1000);
    } catch (error) {
      console.error('Failed to request permissions:', error);
    }
  };

  // Check permissions on mount.
  //
  // Retry while no microphone shows up: right after launch CoreAudio can still
  // report zero input devices (the app is also spawning the sidecar and warming
  // the DB at that moment). The result gates the whole recording transport in
  // page.tsx, so a single failed probe used to hide the Start recording button
  // until the user manually reloaded the window.
  //
  // ponytail: fixed 1s backoff, 5 tries. If a machine is still reporting no mic
  // after 5s that's a real missing-mic state — PermissionWarning's Recheck
  // button covers it from there.
  useEffect(() => {
    let cancelled = false;
    let timer: ReturnType<typeof setTimeout>;
    let attempts = 0;

    const probe = async () => {
      const { hasMicrophone } = await checkPermissions();
      if (!hasMicrophone && !cancelled && ++attempts < 5) {
        timer = setTimeout(probe, 1000);
      }
    };
    probe();

    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, []);

  return {
    ...status,
    checkPermissions,
    requestPermissions,
  };
}
