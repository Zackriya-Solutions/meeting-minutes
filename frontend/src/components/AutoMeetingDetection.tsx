'use client';

import { useEffect } from 'react';
import { listen } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
import { usePathname, useRouter } from 'next/navigation';
import { toast } from 'sonner';
import { useRecordingState } from '@/contexts/RecordingStateContext';
import { useT } from '@/lib/i18n';

type MeetingApp =
  | 'zoom'
  | 'microsoft_teams'
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

const TOAST_ID = 'auto-meeting-detected';
const AUTO_LISTENING_SESSION_KEY = 'autoListeningSessionId';
const AUTO_LISTENING_REPORTED_KEY = 'autoListeningStartReported';
const AUTO_LISTENING_STOP_KEY = 'autoStopRecordingSessionId';

export function AutoMeetingDetection() {
  const pathname = usePathname();
  const router = useRouter();
  const recordingState = useRecordingState();
  const t = useT();

  useEffect(() => {
    if (recordingState.isRecording) {
      toast.dismiss(TOAST_ID);
      const sessionId = sessionStorage.getItem(AUTO_LISTENING_SESSION_KEY);
      const alreadyReported = sessionStorage.getItem(AUTO_LISTENING_REPORTED_KEY) === sessionId;
      if (sessionId && !alreadyReported) {
        sessionStorage.setItem(AUTO_LISTENING_REPORTED_KEY, sessionId);
        invoke('report_auto_listening_start', {
          input: { session_id: sessionId, success: true, failure_reason: null },
        }).catch((error) => {
          console.warn('Failed to persist auto-listening start:', error);
        });
        toast.success(t('Auto-listening started recording'), {
          description: t('Memento will stop and save this meeting after the call signal ends.'),
        });
      }
    }
  }, [recordingState.isRecording, t]);

  useEffect(() => {
    let disposed = false;
    const unlisteners: Array<() => void> = [];

    const appName = (app: MeetingApp): string => {
      switch (app) {
        case 'zoom': return 'Zoom';
        case 'microsoft_teams': return 'Microsoft Teams';
        case 'yandex_telemost': return t('Yandex Telemost');
        case 'salute_jazz': return 'SaluteJazz';
        case 'browser_call': return t('browser call');
      }
    };

    const startRecording = () => {
      toast.dismiss(TOAST_ID);
      if (pathname === '/') {
        window.dispatchEvent(new CustomEvent('start-recording-from-sidebar'));
      } else {
        sessionStorage.setItem('autoStartRecording', 'true');
        router.push('/');
      }
    };

    listen<MeetingDetectedEvent>('auto-meeting-detected', (event) => {
      if (recordingState.isRecording) return;

      const apps = Array.from(new Set(event.payload.apps.map(appName))).join(', ');
      toast.info(t('Meeting detected'), {
        id: TOAST_ID,
        description: apps
          ? `${t('Active call signal')}: ${apps}. ${t('Start recording?')}`
          : t('Start recording?'),
        action: {
          label: t('Start recording'),
          onClick: startRecording,
        },
        cancel: {
          label: t('Not now'),
          onClick: () => undefined,
        },
        // A call is normally detected while the browser owns focus. Keep the in-app prompt
        // until the user makes a choice so it is still present when they return to Memento.
        duration: Infinity,
      });
    }).then((unsubscribe) => {
      if (disposed) unsubscribe();
      else unlisteners.push(unsubscribe);
    }).catch((error) => {
      console.error('Failed to subscribe to meeting detection events:', error);
    });

    listen<AutoListeningEvent>('auto-listening-start-requested', (event) => {
      if (recordingState.isRecording) return;
      const sessionId = event.payload.session_id;
      sessionStorage.setItem(AUTO_LISTENING_SESSION_KEY, sessionId);
      sessionStorage.removeItem(AUTO_LISTENING_REPORTED_KEY);
      sessionStorage.setItem('autoListeningStart', 'true');
      toast.info(t('Meeting detected — starting auto-listening'), { duration: 5000 });

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
  }, [pathname, recordingState.isRecording, router, t]);

  return null;
}
