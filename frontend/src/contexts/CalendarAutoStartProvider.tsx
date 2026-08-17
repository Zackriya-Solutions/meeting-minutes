'use client';

import React, { useEffect } from 'react';
import { useRouter } from 'next/navigation';
import { listen } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';

interface CalendarAutoStartPendingPayload {
  eventId: string;
  title: string;
  /** "ask" | "silent" */
  mode: string;
}

// How long an "ask first" prompt stays cancellable before the recording auto-starts anyway.
const ASK_WINDOW_MS = 45_000;

/**
 * CalendarAutoStartProvider
 *
 * Listens for the 'calendar-auto-start-pending' event emitted by the Rust calendar
 * poller when a synced meeting enters its auto-start window. Mirrors the two existing
 * start-recording triggers so it works regardless of which page the user is on:
 * - sessionStorage flag + navigate to '/' (same mechanism as CalendarSettings' "Record now")
 * - a live 'start-recording-from-sidebar' window event (same mechanism as the tray's
 *   "Start Recording" action), for when the user is already on the home page and the
 *   navigate above would be a no-op.
 */
export function CalendarAutoStartProvider({ children }: { children: React.ReactNode }) {
  const router = useRouter();

  useEffect(() => {
    let unlistenFn: (() => void) | undefined;

    const startAutoRecording = async (eventId: string, title: string) => {
      sessionStorage.setItem('autoStartRecording', 'true');
      sessionStorage.setItem('autoStartMeetingTitle', title);
      router.push('/');
      window.dispatchEvent(new CustomEvent('start-recording-from-sidebar'));

      try {
        await invoke('calendar_confirm_auto_start', { eventId });
      } catch (error) {
        console.error('[CalendarAutoStart] Failed to confirm auto-start:', error);
      }
    };

    const setupListener = async () => {
      try {
        unlistenFn = await listen<CalendarAutoStartPendingPayload>(
          'calendar-auto-start-pending',
          (event) => {
            const { eventId, title, mode } = event.payload;
            console.log('[CalendarAutoStart] Received calendar-auto-start-pending:', event.payload);

            if (mode === 'silent') {
              startAutoRecording(eventId, title);
              return;
            }

            let cancelled = false;
            toast.info(`Meetily will start recording "${title}" shortly`, {
              description: 'From your Google Calendar. Click Cancel to skip this one.',
              duration: ASK_WINDOW_MS,
              action: {
                label: 'Cancel',
                onClick: () => {
                  cancelled = true;
                },
              },
            });

            setTimeout(() => {
              if (!cancelled) {
                startAutoRecording(eventId, title);
              }
            }, ASK_WINDOW_MS);
          }
        );
      } catch (error) {
        console.error('[CalendarAutoStart] Failed to set up event listener:', error);
      }
    };

    setupListener();

    return () => {
      unlistenFn?.();
    };
  }, [router]);

  return <>{children}</>;
}
