// Shows Gemini Live fallback/resume notices as toasts. Fork addition.
'use client';

import { useEffect } from 'react';
import { listen } from '@tauri-apps/api/event';
import { toast } from 'sonner';
import { GeminiLiveStatus } from '@/lib/gemini-live';

export function GeminiLiveStatusToasts() {
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let disposed = false;
    listen<GeminiLiveStatus>('gemini-live-status', ({ payload }) => {
      if (payload.kind === 'resumed') {
        toast.success('Gemini Live', { description: payload.message, duration: 5000 });
      } else {
        toast.warning('Gemini Live', { description: payload.message, duration: 10000 });
      }
    }).then((fn) => {
      if (disposed) fn();
      else unlisten = fn;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);
  return null;
}
