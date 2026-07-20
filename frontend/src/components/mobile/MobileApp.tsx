'use client';

/**
 * Voice Me — mobile shell (Android).
 *
 * Implements the "Voice Me" Claude Design handoff: onboarding, meetings
 * home, live recording, meeting detail (transcript/summary/notes), model
 * manager, and settings, wired to the same Rust/Tauri core as desktop.
 */

import { useCallback, useEffect, useState } from 'react';
import { List, Settings as SettingsIcon, LayoutGrid, FileAudio } from 'lucide-react';
import './mobile.css';
import type { VmAccent, VmMeeting, VmModel, VmScreen, VmTheme } from './types';
import {
  fetchMeetings,
  fetchModels,
  hasTranscriptConfig,
  onModelDownloadComplete,
  onModelDownloadProgress,
  selectWhisperModel,
} from './tauriBridge';
import { OnboardingScreen } from './OnboardingScreen';
import { HomeScreen } from './HomeScreen';
import { RecordingScreen } from './RecordingScreen';
import { MeetingDetailScreen } from './MeetingDetailScreen';
import { ModelsScreen } from './ModelsScreen';
import { SettingsScreen } from './SettingsScreen';
import { ImportAudioScreen } from './ImportAudioScreen';
import { RecordingsScreen } from './RecordingsScreen';

const LS_THEME = 'vm-theme';
const LS_ACCENT = 'vm-accent';
const LS_ONBOARDED = 'vm-onboarded';

function lsGet(key: string): string | null {
  try {
    return typeof window !== 'undefined' ? window.localStorage.getItem(key) : null;
  } catch {
    return null;
  }
}

function lsSet(key: string, value: string) {
  try {
    window.localStorage.setItem(key, value);
  } catch {
    /* ignore */
  }
}

export default function MobileApp() {
  const [theme, setTheme] = useState<VmTheme>('dark');
  const [accent, setAccent] = useState<VmAccent>('teal');
  const [onboarded, setOnboarded] = useState<boolean | null>(null);
  const [screen, setScreen] = useState<VmScreen>('home');
  const [modelsFrom, setModelsFrom] = useState<VmScreen>('home');
  const [meetings, setMeetings] = useState<VmMeeting[]>([]);
  const [models, setModels] = useState<VmModel[]>([]);
  const [activeMeetingId, setActiveMeetingId] = useState<string | null>(null);
  const [detailInitialTab, setDetailInitialTab] = useState<'transcript' | 'summary' | 'notes'>('transcript');

  // Restore persisted appearance + onboarding state
  useEffect(() => {
    const t = lsGet(LS_THEME);
    if (t === 'light' || t === 'dark') setTheme(t);
    const a = lsGet(LS_ACCENT);
    if (a) setAccent(a as VmAccent);
    setOnboarded(lsGet(LS_ONBOARDED) === '1');
  }, []);

  const applyTheme = useCallback((t: VmTheme) => {
    setTheme(t);
    lsSet(LS_THEME, t);
  }, []);

  const applyAccent = useCallback((a: VmAccent) => {
    setAccent(a);
    lsSet(LS_ACCENT, a);
  }, []);

  const reloadMeetings = useCallback(async () => {
    setMeetings(await fetchMeetings());
  }, []);

  const reloadModels = useCallback(async () => {
    setModels(await fetchModels());
  }, []);

  useEffect(() => {
    reloadMeetings();
    reloadModels();
  }, [reloadMeetings, reloadModels]);

  // Self-heal installs that already have a downloaded Whisper model but no
  // saved transcript config (e.g. from before this fix shipped) — otherwise
  // recording fails with a Parakeet-related error since that's the backend's
  // fallback engine when no config row exists.
  useEffect(() => {
    if (models.length === 0) return;
    const downloaded = models.find((m) => m.status === 'downloaded');
    if (!downloaded) return;
    hasTranscriptConfig().then((has) => {
      if (!has) selectWhisperModel(downloaded.name);
    });
  }, [models]);

  // Global model-download progress stream keeps every screen in sync
  useEffect(() => {
    let unProgress: (() => void) | undefined;
    let unComplete: (() => void) | undefined;
    onModelDownloadProgress((name, progress) => {
      setModels((ms) =>
        ms.map((m) =>
          m.name === name ? { ...m, status: 'downloading', progress } : m
        )
      );
    }).then((u) => (unProgress = u));
    onModelDownloadComplete((name) => {
      setModels((ms) =>
        ms.map((m) =>
          m.name === name ? { ...m, status: 'downloaded', progress: 100 } : m
        )
      );
      // The Rust recording pipeline defaults to the Parakeet engine (which
      // isn't set up on Android) until a local transcript config exists.
      // Auto-select the first downloaded Whisper model so recording works
      // without an extra "choose your engine" step; don't clobber a config
      // the user already picked.
      hasTranscriptConfig().then((has) => {
        if (!has) selectWhisperModel(name);
      });
    }).then((u) => (unComplete = u));
    return () => {
      unProgress?.();
      unComplete?.();
    };
  }, []);

  const hasModel = models.some((m) => m.status === 'downloaded');

  const openMeeting = useCallback((id: string, tab: 'transcript' | 'summary' | 'notes' = 'transcript') => {
    setActiveMeetingId(id);
    setDetailInitialTab(tab);
    setScreen('detail');
  }, []);

  const finishOnboarding = useCallback(() => {
    lsSet(LS_ONBOARDED, '1');
    setOnboarded(true);
    setScreen('home');
    reloadModels();
    reloadMeetings();
  }, [reloadMeetings, reloadModels]);

  const onRecordingStopped = useCallback(async () => {
    const fresh = await fetchMeetings();
    setMeetings(fresh);
    if (fresh.length > 0) {
      openMeeting(fresh[0].id, 'summary');
    } else {
      setScreen('home');
    }
  }, [openMeeting]);

  const gotoModels = useCallback(
    (from: VmScreen) => {
      setModelsFrom(from);
      setScreen('models');
    },
    []
  );

  if (onboarded === null) {
    // Avoid a flash of the wrong screen while localStorage loads
    return <div className="vm-app" data-theme={theme} data-accent={accent} />;
  }

  const showTabBar = ['home', 'recordings', 'models', 'settings'].includes(screen);

  return (
    <div className="vm-app" data-theme={theme} data-accent={accent}>
      {!onboarded ? (
        <OnboardingScreen models={models} onFinished={finishOnboarding} />
      ) : (
        <>
          {screen === 'home' && (
            <HomeScreen
              meetings={meetings}
              hasModel={hasModel}
              onOpenMeeting={(id) => openMeeting(id)}
              onStartRecording={() => setScreen('recording')}
              onOpenModels={() => gotoModels('home')}
              onOpenImport={() => setScreen('import')}
            />
          )}
          {screen === 'recording' && (
            <RecordingScreen
              onStopped={onRecordingStopped}
              onDiscard={() => setScreen('home')}
            />
          )}
          {screen === 'import' && (
            <ImportAudioScreen
              models={models}
              onBack={() => setScreen('home')}
              onImported={(meetingId) => {
                reloadMeetings();
                openMeeting(meetingId, 'summary');
              }}
            />
          )}
          {screen === 'recordings' && <RecordingsScreen />}
          {screen === 'detail' && activeMeetingId && (
            <MeetingDetailScreen
              meetingId={activeMeetingId}
              initialTab={detailInitialTab}
              onBack={() => {
                reloadMeetings();
                setScreen('home');
              }}
              onOpenSettings={() => setScreen('settings')}
            />
          )}
          {screen === 'models' && (
            <ModelsScreen
              models={models}
              onModelsChanged={reloadModels}
              onBack={() => setScreen(modelsFrom === 'settings' ? 'settings' : 'home')}
            />
          )}
          {screen === 'settings' && (
            <SettingsScreen
              theme={theme}
              accent={accent}
              models={models}
              onSetTheme={applyTheme}
              onSetAccent={applyAccent}
              onOpenModels={() => gotoModels('settings')}
            />
          )}
          {showTabBar && (
            <div className="tabbar">
              <button
                className={`tabitem ${screen === 'home' ? 'on' : ''}`}
                onClick={() => setScreen('home')}
              >
                <List size={22} strokeWidth={2} />
                <span>Meetings</span>
              </button>
              <button
                className={`tabitem ${screen === 'recordings' ? 'on' : ''}`}
                onClick={() => setScreen('recordings')}
              >
                <FileAudio size={22} strokeWidth={2} />
                <span>Recordings</span>
              </button>
              <button
                className={`tabitem ${screen === 'models' ? 'on' : ''}`}
                onClick={() => gotoModels('home')}
              >
                <LayoutGrid size={22} strokeWidth={2} />
                <span>Models</span>
              </button>
              <button
                className={`tabitem ${screen === 'settings' ? 'on' : ''}`}
                onClick={() => setScreen('settings')}
              >
                <SettingsIcon size={22} strokeWidth={1.8} />
                <span>Settings</span>
              </button>
            </div>
          )}
        </>
      )}
    </div>
  );
}
