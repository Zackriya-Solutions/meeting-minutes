'use client';

import { useEffect } from 'react';
import { listen } from '@tauri-apps/api/event';
import { usePathname, useRouter } from 'next/navigation';
import { toast } from 'sonner';
import { useRecordingState } from '@/contexts/RecordingStateContext';
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

const TOAST_ID = 'auto-meeting-detected';

export function AutoMeetingDetection() {
  const pathname = usePathname();
  const router = useRouter();
  const recordingState = useRecordingState();
  const t = useT();

  useEffect(() => {
    if (recordingState.isRecording) {
      toast.dismiss(TOAST_ID);
    }
  }, [recordingState.isRecording]);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;

    const appName = (app: MeetingApp): string => {
      switch (app) {
        case 'zoom': return 'Zoom';
        case 'microsoft_teams': return 'Microsoft Teams';
        case 'telegram': return 'Telegram';
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
        duration: 30_000,
      });
    }).then((unsubscribe) => {
      if (disposed) unsubscribe();
      else unlisten = unsubscribe;
    }).catch((error) => {
      console.error('Failed to subscribe to meeting detection events:', error);
    });

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [pathname, recordingState.isRecording, router, t]);

  return null;
}
