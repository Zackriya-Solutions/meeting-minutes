'use client';

import { useEffect } from 'react';
import { listen } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
import { usePathname, useRouter } from 'next/navigation';
import { toast } from 'sonner';
import { useRecordingState } from '@/contexts/RecordingStateContext';
import { useSidebar } from '@/components/Sidebar/SidebarProvider';
import { useT } from '@/lib/i18n';

type MeetingApp =
  | 'zoom'
  | 'microsoft_teams'
  | 'telegram'
  | 'yandex_telemost'
  | 'salute_jazz'
  | 'browser_call';

interface MeetingDetectedEvent {
  apps: MeetingApp[];
  source: 'native_process' | 'microphone_activity';
}

interface AutoListeningEvent extends MeetingDetectedEvent {
  session_id: string;
}

interface AutoListeningStopEvent {
  session_id: string;
}

interface BackgroundCaptureSavedEvent {
  meeting_id: string;
  title: string;
  duration_seconds: number;
  /** False when the audio was written but the meeting row could not be inserted. */
  registered: boolean;
}

const TOAST_ID = 'auto-meeting-detected';
const AUTO_LISTENING_SESSION_KEY = 'autoListeningSessionId';
const AUTO_LISTENING_REPORTED_KEY = 'autoListeningStartReported';
const AUTO_LISTENING_STOP_KEY = 'autoStopRecordingSessionId';

export function AutoMeetingDetection() {
  const pathname = usePathname();
  const router = useRouter();
  const recordingState = useRecordingState();
  const { refetchMeetings } = useSidebar();
  const t = useT();

  useEffect(() => {
    if (recordingState.isRecording) {
      const sessionId = sessionStorage.getItem(AUTO_LISTENING_SESSION_KEY);
      const alreadyReported = sessionStorage.getItem(AUTO_LISTENING_REPORTED_KEY) === sessionId;
      if (sessionId && !alreadyReported) {
        sessionStorage.setItem(AUTO_LISTENING_REPORTED_KEY, sessionId);
        invoke('report_auto_listening_start', {
          input: { session_id: sessionId, success: true, failure_reason: null },
        }).catch((error) => {
          console.warn('Failed to persist auto-listening start:', error);
        });
      }
    }
  }, [recordingState.isRecording, t]);

  useEffect(() => {
    let disposed = false;
    const unlisteners: Array<() => void> = [];

    listen<AutoListeningEvent>('auto-listening-start-requested', (event) => {
      if (recordingState.isRecording) return;
      const sessionId = event.payload.session_id;
      sessionStorage.setItem(AUTO_LISTENING_SESSION_KEY, sessionId);
      sessionStorage.removeItem(AUTO_LISTENING_REPORTED_KEY);
      sessionStorage.setItem('autoListeningStart', 'true');

      if (pathname === '/') {
        sessionStorage.removeItem('autoStartRecording');
        window.dispatchEvent(new CustomEvent('start-recording-from-sidebar'));
      } else {
        sessionStorage.setItem('autoStartRecording', 'true');
        router.push('/');
      }
    }).then((unsubscribe) => {
      if (disposed) unsubscribe();
      else unlisteners.push(unsubscribe);
    }).catch((error) => {
      console.error('Failed to subscribe to auto-listening start events:', error);
    });

    // Background auto-recording runs entirely in Rust, so the webview only learns
    // about it when a capture lands: pull the new meeting into the sidebar list.
    listen<BackgroundCaptureSavedEvent>('background-capture-saved', (event) => {
      const { title, registered } = event.payload;
      if (registered) {
        refetchMeetings();
        toast.success(t('Meeting recorded in the background'), {
          description: `${title}. ${t('Open it and press Enhance to transcribe.')}`,
        });
      } else {
        toast.warning(t('Background recording saved to disk only'), {
          description: `${title}. ${t('Import it from your recordings folder to transcribe.')}`,
        });
      }
    }).then((unsubscribe) => {
      if (disposed) unsubscribe();
      else unlisteners.push(unsubscribe);
    }).catch((error) => {
      console.error('Failed to subscribe to background capture events:', error);
    });

    listen<AutoListeningStopEvent>('auto-listening-stop-requested', (event) => {
      const sessionId = event.payload.session_id;
      if (sessionStorage.getItem(AUTO_LISTENING_SESSION_KEY) !== sessionId) return;
      sessionStorage.setItem(AUTO_LISTENING_STOP_KEY, sessionId);
      if (pathname === '/') {
        window.dispatchEvent(new CustomEvent('stop-recording-from-auto-listening'));
      } else {
        router.push('/');
      }
    }).then((unsubscribe) => {
      if (disposed) unsubscribe();
      else unlisteners.push(unsubscribe);
    }).catch((error) => {
      console.error('Failed to subscribe to auto-listening stop events:', error);
    });

    return () => {
      disposed = true;
      unlisteners.forEach((unlisten) => unlisten());
    };
  }, [pathname, recordingState.isRecording, refetchMeetings, router, t]);

  return null;
}
