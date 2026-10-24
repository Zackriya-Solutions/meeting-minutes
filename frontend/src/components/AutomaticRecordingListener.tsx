'use client';

import { useEffect } from 'react';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { useSidebar } from '@/components/Sidebar/SidebarProvider';
import { toast } from 'sonner';

export function AutomaticRecordingListener() {
  const { setIsMeetingActive } = useSidebar();
  useEffect(() => {
    let disposed = false;
    const listeners: UnlistenFn[] = [];
    const register = async () => {
      for (const promise of [
        listen('automatic-recording-started', () => {
          setIsMeetingActive(true);
          toast.success('Call detected. Recording started.', { description: 'You can stop recording from the menu bar or Meetily.' });
        }),
        listen<string>('automatic-recording-error', event => toast.error(event.payload)),
      ]) {
        const unlisten = await promise;
        if (disposed) unlisten(); else listeners.push(unlisten);
      }
    };
    void register().catch(() => { /* Native events are unavailable in browser previews. */ });
    return () => { disposed = true; listeners.forEach(unlisten => unlisten()); };
  }, [setIsMeetingActive]);
  return null;
}
