'use client';

import { useEffect, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
import { usePathname, useRouter } from 'next/navigation';
import { useRecordingState } from '@/contexts/RecordingStateContext';
import { useT } from '@/lib/i18n';
import {
  MeetingDetectionBanner,
  type MeetingDetectionBannerState,
} from '@/components/MeetingDetectionBanner';

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

const AUTO_LISTENING_SESSION_KEY = 'autoListeningSessionId';
const AUTO_LISTENING_REPORTED_KEY = 'autoListeningStartReported';
const AUTO_LISTENING_STOP_KEY = 'autoStopRecordingSessionId';

interface DetectionBannerData {
  apps: MeetingApp[];
  state: MeetingDetectionBannerState;
}

export function AutoMeetingDetection() {
  const pathname = usePathname();
  const router = useRouter();
  const recordingState = useRecordingState();
  const t = useT();
  const [banner, setBanner] = useState<DetectionBannerData | null>(null);

  const appName = (app: MeetingApp): string => {
    switch (app) {
      case 'zoom': return 'Zoom';
      case 'microsoft_teams': return 'Microsoft Teams';
      case 'yandex_telemost': return t('Yandex Telemost');
      case 'salute_jazz': return 'SaluteJazz';
      case 'browser_call': return 'браузере';
    }
  };

  const startRecording = () => {
    setBanner((current) => current ? { ...current, state: 'starting' } : current);
    if (pathname === '/') {
      window.dispatchEvent(new CustomEvent('start-recording-from-sidebar'));
    } else {
      sessionStorage.setItem('autoStartRecording', 'true');
      router.push('/');
    }
  };

  useEffect(() => {
    if (recordingState.isRecording) {
      setBanner((current) => current ? { ...current, state: 'recording' } : current);
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

    listen<MeetingDetectedEvent>('auto-meeting-detected', (event) => {
      if (recordingState.isRecording) return;
      setBanner({ apps: event.payload.apps, state: 'suggestion' });
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
      setBanner({ apps: event.payload.apps, state: 'starting' });

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
      setBanner(null);
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

  return (
    <MeetingDetectionBanner
      open={banner !== null}
      state={banner?.state ?? 'suggestion'}
      appNames={banner ? Array.from(new Set(banner.apps.map(appName))) : []}
      onPrimaryAction={banner?.state === 'recording' ? () => router.push('/recording') : startRecording}
      onDismiss={() => setBanner(null)}
    />
  );
}
