'use client';

import React, { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { PermissionStatus, OnboardingPermissions } from '@/types/onboarding';
import {
  DEFAULT_SUMMARY_MODEL,
  isOfferedSummaryModel,
} from '@/lib/onboarding-summary-model';

interface OnboardingStatus {
  version: string;
  completed: boolean;
  current_step: number;
  model_status: {
    /**
     * Whether the local transcription model is installed. The key is still `parakeet` for
     * compatibility with statuses saved before GigaAM replaced Parakeet as the engine —
     * renaming it would make every existing install look un-onboarded.
     */
    parakeet: string;
    summary: string;
    selected_summary_model?: string;
  };
  last_updated: string;
}

/**
 * The models a fresh install needs, downloaded as one package.
 *
 * Transcription (GigaAM) and speaker recognition arrive from two different backends with
 * their own progress events, but the user asked for one download, so they are presented and
 * driven as one: a single byte total, a single bar, one retry.
 */
export type ModelPackStatus = 'idle' | 'checking' | 'downloading' | 'ready' | 'error';

export interface ModelPackState {
  status: ModelPackStatus;
  /** 0..100 over the package's combined bytes. */
  percent: number;
  downloadedMb: number;
  totalMb: number;
  transcriptionReady: boolean;
  speakersReady: boolean;
  error?: string;
}

/** Shown until `gigaam_status` reports the selected variant's real size. */
const TRANSCRIPTION_FALLBACK_MB = 987;
/** Shown until `diarization_status` reports the real combined size. */
const SPEAKERS_FALLBACK_MB = 34;

const INITIAL_PACK: ModelPackState = {
  status: 'checking',
  percent: 0,
  downloadedMb: 0,
  totalMb: TRANSCRIPTION_FALLBACK_MB + SPEAKERS_FALLBACK_MB,
  transcriptionReady: false,
  speakersReady: false,
};

export interface OnboardingContextType {
  currentStep: number;
  /** Product, permissions, ready. */
  totalSteps: number;
  isMac: boolean;
  /** False until the saved status has been read — the gate must not flash. */
  statusLoaded: boolean;
  shouldRun: boolean;
  modelPack: ModelPackState;
  transcriptionLabel: string;
  summaryModel: string;
  setSummaryModel: (model: string) => void;
  permissions: OnboardingPermissions;
  permissionsSkipped: boolean;
  goToStep: (step: number) => void;
  goNext: () => void;
  goPrevious: () => void;
  setPermissionStatus: (permission: keyof OnboardingPermissions, status: PermissionStatus) => void;
  setPermissionsSkipped: (skipped: boolean) => void;
  startModelPack: () => Promise<void>;
  /** Resolves to the example meeting's id when one was seeded. */
  completeOnboarding: () => Promise<string | null>;
}

export const OnboardingContext = createContext<OnboardingContextType | undefined>(undefined);

export function OnboardingProvider({ children }: { children: React.ReactNode }) {
  const [currentStep, setCurrentStep] = useState(1);
  const [isMac, setIsMac] = useState(false);
  const [statusLoaded, setStatusLoaded] = useState(false);
  const [shouldRun, setShouldRun] = useState(false);
  const [summaryModel, setSummaryModelState] = useState<string>(DEFAULT_SUMMARY_MODEL);
  const [modelPack, setModelPack] = useState<ModelPackState>(INITIAL_PACK);
  const [transcriptionLabel, setTranscriptionLabel] = useState('GigaAM v3');

  const [permissions, setPermissions] = useState<OnboardingPermissions>({
    microphone: 'not_determined',
    systemAudio: 'not_determined',
    screenRecording: 'not_determined',
  });
  const [permissionsSkipped, setPermissionsSkipped] = useState(false);

  const totalSteps = 3;

  // Byte counters, kept outside React state so progress events can be merged without
  // depending on the previous render's numbers.
  const bytes = useRef({
    transcriptionMb: 0,
    transcriptionTotalMb: TRANSCRIPTION_FALLBACK_MB,
    speakersTotalMb: SPEAKERS_FALLBACK_MB,
    transcriptionReady: false,
    speakersReady: false,
  });
  const packStartedRef = useRef(false);
  const saveTimeoutRef = useRef<NodeJS.Timeout>();
  const isCompletingRef = useRef(false);

  const publishPack = useCallback((patch: Partial<ModelPackState> = {}) => {
    const b = bytes.current;
    const total = b.transcriptionTotalMb + b.speakersTotalMb;
    // Speaker recognition is ~3% of the package and arrives as two files whose progress
    // restarts per file. Counting it only once finished keeps the bar monotonic; the caption
    // names what is still running.
    const downloaded =
      (b.transcriptionReady ? b.transcriptionTotalMb : b.transcriptionMb) +
      (b.speakersReady ? b.speakersTotalMb : 0);

    setModelPack((prev) => ({
      ...prev,
      totalMb: total,
      downloadedMb: Math.min(downloaded, total),
      percent: total > 0 ? Math.min(100, (downloaded / total) * 100) : 0,
      transcriptionReady: b.transcriptionReady,
      speakersReady: b.speakersReady,
      ...patch,
    }));
  }, []);

  useEffect(() => {
    const detectPlatform = async () => {
      try {
        const { platform } = await import('@tauri-apps/plugin-os');
        setIsMac(platform() === 'macos');
      } catch {
        setIsMac(navigator.userAgent.includes('Mac'));
      }
    };
    void detectPlatform();
  }, []);

  // Whether onboarding runs at all, plus the step to resume on.
  useEffect(() => {
    const load = async () => {
      try {
        const [needed, status] = await Promise.all([
          invoke<boolean>('onboarding_should_run'),
          invoke<OnboardingStatus | null>('get_onboarding_status').catch(() => null),
        ]);

        setShouldRun(needed);
        const savedModel = status?.model_status?.selected_summary_model;
        if (savedModel && isOfferedSummaryModel(savedModel)) {
          setSummaryModelState(savedModel);
        }
        if (needed && status && status.current_step >= 1) {
          setCurrentStep(Math.min(status.current_step, 3));
        }
      } catch (error) {
        console.error('[OnboardingContext] Failed to read onboarding status:', error);
        // A status we cannot read is not a reason to block the app.
        setShouldRun(false);
      } finally {
        setStatusLoaded(true);
      }
    };
    void load();
  }, []);

  // What the package contains and what is already on disk.
  useEffect(() => {
    if (!shouldRun) return;

    const checkPresence = async () => {
      // A download can already be running — the app was restarted mid-setup, or Settings
      // started one. Saying `idle` then would make the step re-issue the same request.
      let inFlight = false;

      try {
        const status = await invoke<{
          selected: string;
          model_present: boolean;
          downloading?: boolean;
          variants: { id: string; label: string; size_mb: number }[];
        }>('gigaam_status');
        const selected = status.variants?.find((variant) => variant.id === status.selected);
        if (selected) {
          setTranscriptionLabel(selected.label);
          bytes.current.transcriptionTotalMb = selected.size_mb || TRANSCRIPTION_FALLBACK_MB;
        }
        bytes.current.transcriptionReady = Boolean(status.model_present);
        inFlight = inFlight || Boolean(status.downloading);
      } catch (error) {
        console.warn('[OnboardingContext] gigaam_status failed:', error);
      }

      try {
        const status = await invoke<{
          available: boolean;
          download_mb: number;
          downloading: boolean;
        }>('diarization_status');
        bytes.current.speakersTotalMb = status.download_mb || SPEAKERS_FALLBACK_MB;
        bytes.current.speakersReady = status.available;
        inFlight = inFlight || status.downloading;
      } catch (error) {
        console.warn('[OnboardingContext] diarization_status failed:', error);
      }

      const ready = bytes.current.transcriptionReady && bytes.current.speakersReady;
      if (inFlight && !ready) packStartedRef.current = true;
      publishPack({ status: ready ? 'ready' : inFlight ? 'downloading' : 'idle' });
    };

    void checkPresence();
  }, [shouldRun, publishPack]);

  // Transcription model progress. `extracting` reports no byte counts, so the bar holds its
  // last value instead of snapping back to zero.
  useEffect(() => {
    const unlistenProgress = listen<{
      downloaded: number;
      total: number;
      percent: number;
      stage: string;
    }>('gigaam-download-progress', (event) => {
      const { downloaded, total, stage } = event.payload;
      if (stage === 'downloading') {
        bytes.current.transcriptionMb = downloaded / (1024 * 1024);
        if (total > 0) bytes.current.transcriptionTotalMb = total / (1024 * 1024);
      }
      publishPack({ status: 'downloading', error: undefined });
    });

    const unlistenReady = listen('gigaam-ready', () => {
      bytes.current.transcriptionReady = true;
      publishPack(
        bytes.current.speakersReady ? { status: 'ready', error: undefined } : { error: undefined },
      );
    });

    const unlistenError = listen<string>('gigaam-download-error', (event) => {
      packStartedRef.current = false;
      publishPack({ status: 'error', error: event.payload });
    });

    const unlistenSpeakersReady = listen('diarization-ready', () => {
      bytes.current.speakersReady = true;
      publishPack(bytes.current.transcriptionReady ? { status: 'ready' } : {});
    });

    const unlistenSpeakersError = listen<string>('diarization-download-error', (event) => {
      // Speaker recognition is the smaller, less critical half of the package: report it,
      // but do not present the whole download as failed while transcription is still coming.
      console.warn('[OnboardingContext] speaker models failed:', event.payload);
      if (bytes.current.transcriptionReady) {
        // Nothing else is in flight, so "Повторить" has to be able to start a new attempt.
        packStartedRef.current = false;
        publishPack({ status: 'error', error: event.payload });
      }
    });

    return () => {
      unlistenProgress.then((fn) => fn());
      unlistenReady.then((fn) => fn());
      unlistenError.then((fn) => fn());
      unlistenSpeakersReady.then((fn) => fn());
      unlistenSpeakersError.then((fn) => fn());
    };
  }, [publishPack]);

  /**
   * Fetch everything the package is missing. Both backends skip files already on disk, so
   * this doubles as retry: an interrupted download resumes rather than starting over.
   */
  const startModelPack = useCallback(async () => {
    if (packStartedRef.current) return;
    if (bytes.current.transcriptionReady && bytes.current.speakersReady) {
      publishPack({ status: 'ready' });
      return;
    }

    packStartedRef.current = true;
    publishPack({ status: 'downloading', error: undefined });

    const failures: string[] = [];
    if (!bytes.current.transcriptionReady) {
      try {
        await invoke('gigaam_download_model');
      } catch (error) {
        // "already in progress" means someone else is doing the work we wanted done.
        const message = String(error);
        if (!message.toLowerCase().includes('progress')) failures.push(message);
      }
    }
    if (!bytes.current.speakersReady) {
      try {
        await invoke('download_diarization_models');
      } catch (error) {
        console.warn('[OnboardingContext] speaker model download failed to start:', error);
      }
    }

    if (failures.length > 0) {
      packStartedRef.current = false;
      publishPack({ status: 'error', error: failures[0] });
    }
  }, [publishPack]);

  const setSummaryModel = useCallback((model: string) => {
    if (!isOfferedSummaryModel(model)) return;
    setSummaryModelState(model);
  }, []);

  const saveOnboardingStatus = useCallback(async () => {
    if (isCompletingRef.current) return;
    try {
      await invoke('save_onboarding_status_cmd', {
        status: {
          version: '1.0',
          completed: false,
          current_step: currentStep,
          model_status: {
            parakeet: modelPack.transcriptionReady ? 'downloaded' : 'not_downloaded',
            summary: 'cloud',
            selected_summary_model: summaryModel,
          },
          last_updated: new Date().toISOString(),
        },
      });
    } catch (error) {
      console.error('[OnboardingContext] Failed to save onboarding status:', error);
    }
  }, [currentStep, modelPack.transcriptionReady, summaryModel]);

  // Persist progress so a restart mid-setup resumes where it stopped.
  useEffect(() => {
    if (!shouldRun || !statusLoaded) return;
    if (saveTimeoutRef.current) clearTimeout(saveTimeoutRef.current);
    saveTimeoutRef.current = setTimeout(() => {
      void saveOnboardingStatus();
    }, 1000);

    return () => {
      if (saveTimeoutRef.current) clearTimeout(saveTimeoutRef.current);
    };
  }, [shouldRun, statusLoaded, saveOnboardingStatus]);

  const completeOnboarding = useCallback(async () => {
    isCompletingRef.current = true;
    if (saveTimeoutRef.current) {
      clearTimeout(saveTimeoutRef.current);
      saveTimeoutRef.current = undefined;
    }

    try {
      const demoMeetingId = await invoke<string | null>('complete_onboarding', {
        model: summaryModel,
      });
      console.log('[OnboardingContext] Onboarding completed with model:', summaryModel);
      // The flag stays set on success. Download progress keeps arriving after this point, and
      // any one of those events could otherwise schedule a debounced save that writes
      // `completed: false` back over the completion just recorded.
      return demoMeetingId ?? null;
    } catch (error) {
      isCompletingRef.current = false;
      throw error;
    }
  }, [summaryModel]);

  const setPermissionStatus = useCallback(
    (permission: keyof OnboardingPermissions, status: PermissionStatus) => {
      setPermissions((prev) => ({ ...prev, [permission]: status }));
    },
    [],
  );

  const goToStep = useCallback(
    (step: number) => setCurrentStep(Math.max(1, Math.min(step, 3))),
    [],
  );
  const goNext = useCallback(() => setCurrentStep((prev) => Math.min(prev + 1, 3)), []);
  const goPrevious = useCallback(() => setCurrentStep((prev) => Math.max(prev - 1, 1)), []);

  const value = useMemo<OnboardingContextType>(
    () => ({
      currentStep,
      totalSteps,
      isMac,
      statusLoaded,
      shouldRun,
      modelPack,
      transcriptionLabel,
      summaryModel,
      setSummaryModel,
      permissions,
      permissionsSkipped,
      goToStep,
      goNext,
      goPrevious,
      setPermissionStatus,
      setPermissionsSkipped,
      startModelPack,
      completeOnboarding,
    }),
    [
      currentStep,
      totalSteps,
      isMac,
      statusLoaded,
      shouldRun,
      modelPack,
      transcriptionLabel,
      summaryModel,
      setSummaryModel,
      permissions,
      permissionsSkipped,
      goToStep,
      goNext,
      goPrevious,
      setPermissionStatus,
      startModelPack,
      completeOnboarding,
    ],
  );

  return <OnboardingContext.Provider value={value}>{children}</OnboardingContext.Provider>;
}

export function useOnboarding() {
  const context = useContext(OnboardingContext);
  if (!context) {
    throw new Error('useOnboarding must be used within OnboardingProvider');
  }
  return context;
}
