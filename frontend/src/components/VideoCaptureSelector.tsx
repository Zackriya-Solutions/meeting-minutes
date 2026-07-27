'use client';

import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { Monitor, Video, VideoOff } from 'lucide-react';
import { useEffect, useState } from 'react';
import { toast } from 'sonner';

type VideoCaptureMode = 'off' | 'screen' | 'window' | 'browser_tab';

interface CaptureWindow {
  id: number;
  app: string;
  title: string;
}

interface VideoCaptureStatus {
  mode: VideoCaptureMode;
  bridge_connected: boolean;
  tab_armed: boolean;
  tab_title: string | null;
  recording: boolean;
}

const STORAGE_KEY = 'meetily-video-capture-mode';
const WINDOW_STORAGE_KEY = 'meetily-video-capture-window';

export function VideoCaptureSelector({ disabled }: { disabled: boolean }) {
  const [mode, setMode] = useState<VideoCaptureMode>('off');
  const [status, setStatus] = useState<VideoCaptureStatus | null>(null);
  const [windows, setWindows] = useState<CaptureWindow[]>([]);
  const [windowId, setWindowId] = useState<number | null>(null);

  useEffect(() => {
    const stored = localStorage.getItem(STORAGE_KEY) as VideoCaptureMode | null;
    const storedWindowId = Number(localStorage.getItem(WINDOW_STORAGE_KEY)) || null;
    const initial = stored && ['off', 'screen', 'window', 'browser_tab'].includes(stored)
      ? stored
      : 'off';
    const validInitial = initial === 'window' && !storedWindowId ? 'off' : initial;
    setMode(validInitial);
    setWindowId(storedWindowId);
    invoke('set_video_capture_mode', {
      mode: validInitial,
      windowId: validInitial === 'window' ? storedWindowId : null,
    }).catch(console.error);
    invoke<CaptureWindow[]>('list_capture_windows')
      .then(setWindows)
      .catch((error) => console.error('Failed to list capture windows', error));
  }, []);

  useEffect(() => {
    const unlisten = listen<string>('video-capture-error', (event) => {
      toast.error('Meeting audio was saved, but video capture failed', {
        description: event.payload,
        duration: 8000,
      });
    });
    return () => {
      unlisten.then((stopListening) => stopListening());
    };
  }, []);

  useEffect(() => {
    let active = true;
    const refresh = async () => {
      try {
        const next = await invoke<VideoCaptureStatus>('get_video_capture_status');
        if (active) setStatus(next);
      } catch (error) {
        console.error('Failed to read video capture status', error);
      }
    };
    refresh();
    const timer = window.setInterval(refresh, 1000);
    return () => {
      active = false;
      window.clearInterval(timer);
    };
  }, []);

  const updateMode = async (next: VideoCaptureMode) => {
    const selectedWindowId = next === 'window'
      ? windowId || windows[0]?.id || null
      : null;
    try {
      await invoke('set_video_capture_mode', { mode: next, windowId: selectedWindowId });
      setMode(next);
      if (selectedWindowId) {
        setWindowId(selectedWindowId);
        localStorage.setItem(WINDOW_STORAGE_KEY, String(selectedWindowId));
      }
      localStorage.setItem(STORAGE_KEY, next);
      if (next === 'screen') {
        toast.info('The macOS display or area picker will open when recording starts');
      } else if (next === 'window') {
        toast.info('The selected window will be recorded for the whole meeting');
      } else if (next === 'browser_tab') {
        toast.info('Focus a tab and click the Meetily Tab Capture extension to arm it');
      }
    } catch (error) {
      toast.error('Could not change video source', { description: String(error) });
    }
  };

  const updateWindow = async (nextWindowId: number) => {
    try {
      await invoke('set_video_capture_mode', { mode: 'window', windowId: nextWindowId });
      setWindowId(nextWindowId);
      localStorage.setItem(WINDOW_STORAGE_KEY, String(nextWindowId));
    } catch (error) {
      toast.error('Could not select that window', { description: String(error) });
    }
  };

  const Icon = mode === 'off' ? VideoOff : ['screen', 'window'].includes(mode) ? Monitor : Video;
  const browserReady = status?.bridge_connected && status?.tab_armed;

  return (
    <div className="flex items-center gap-2 border-r border-gray-200 px-3">
      <Icon size={17} className={mode === 'off' ? 'text-gray-400' : 'text-blue-600'} />
      <div className="flex flex-col">
        <label htmlFor="video-capture-mode" className="sr-only">Video capture source</label>
        <select
          id="video-capture-mode"
          value={mode}
          disabled={disabled}
          onChange={(event) => updateMode(event.target.value as VideoCaptureMode)}
          className="max-w-[150px] bg-transparent text-xs font-medium text-gray-700 outline-none disabled:opacity-60"
          title="Video capture source"
        >
          <option value="off">Audio only</option>
          <option value="window">Exact window</option>
          <option value="screen">Display / area</option>
          <option value="browser_tab">Browser tab</option>
        </select>
        {mode === 'window' && (
          <select
            value={windowId || ''}
            disabled={disabled || windows.length === 0}
            onChange={(event) => updateWindow(Number(event.target.value))}
            className="max-w-[180px] bg-transparent text-[10px] text-blue-600 outline-none disabled:text-amber-600"
            title="Window to record"
          >
            {windows.length === 0 && <option value="">No windows found</option>}
            {windows.map((window) => (
              <option key={window.id} value={window.id}>
                {window.app}: {window.title}
              </option>
            ))}
          </select>
        )}
        {mode === 'browser_tab' && (
          <span
            className={`max-w-[150px] truncate text-[10px] ${
              browserReady ? 'text-emerald-600' : 'text-amber-600'
            }`}
            title={status?.tab_title || undefined}
          >
            {browserReady ? status?.tab_title || 'Tab armed' : 'Click extension to arm'}
          </span>
        )}
      </div>
    </div>
  );
}
