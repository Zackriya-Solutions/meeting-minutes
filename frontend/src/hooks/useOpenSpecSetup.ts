import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useI18n } from '@/hooks/useI18n';

export type OpenSpecSetupDecision = 'unresolved' | 'installed' | 'skipped';

export type OpenSpecSetupPhase =
  | 'checking'
  | 'idle'
  | 'installing'
  | 'installed'
  | 'skipped'
  | 'error';

interface OpenSpecSetupStatusPayload {
  decision: OpenSpecSetupDecision;
  nodeAvailable: boolean;
  npmAvailable: boolean;
  openspecAvailable: boolean;
}

interface OpenSpecSetupProgressEvent {
  stage: 'checking' | 'downloading_node' | 'extracting_node' | 'installing_openspec' | 'done' | 'error';
  message: string;
  percent?: number | null;
}

const MAX_LOG_LINES = 200;

/**
 * Encapsulates the "check / install / skip" state machine for the OpenSpec
 * CLI setup flow (see frontend/src-tauri/src/openspec/setup.rs). Reusable by
 * both the onboarding-adjacent banner and, in the future, any settings-panel
 * entry point that wants to offer the same install action.
 */
export function useOpenSpecSetup() {
  const { t } = useI18n();
  const [phase, setPhase] = useState<OpenSpecSetupPhase>('checking');
  const [logLines, setLogLines] = useState<string[]>([]);
  const [percent, setPercent] = useState<number | null>(null);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const installingRef = useRef(false);

  const appendLog = useCallback((line: string) => {
    setLogLines((prev) => {
      const next = [...prev, line];
      return next.length > MAX_LOG_LINES ? next.slice(next.length - MAX_LOG_LINES) : next;
    });
  }, []);

  const refreshStatus = useCallback(async () => {
    try {
      const status = await invoke<OpenSpecSetupStatusPayload>('check_openspec_setup_status');

      if (status.decision === 'skipped') {
        setPhase('skipped');
        return status;
      }

      if (status.decision === 'installed' && status.openspecAvailable) {
        setPhase('installed');
        return status;
      }

      if (status.openspecAvailable) {
        // Available on the system but the decision was never persisted
        // (e.g. user installed it manually outside the app) - don't nag.
        setPhase('installed');
        return status;
      }

      setPhase('idle');
      return status;
    } catch (error) {
      console.error('[useOpenSpecSetup] Failed to check setup status:', error);
      setPhase('idle');
      return null;
    }
  }, []);

  useEffect(() => {
    refreshStatus();
  }, [refreshStatus]);

  useEffect(() => {
    const unlisten = listen<OpenSpecSetupProgressEvent>('openspec-setup-progress', (event) => {
      const { stage, message, percent: eventPercent } = event.payload;
      appendLog(message);
      if (typeof eventPercent === 'number') {
        setPercent(eventPercent);
      }
      if (stage === 'error') {
        setErrorMessage(message);
      }
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  }, [appendLog]);

  const install = useCallback(async () => {
    if (installingRef.current) return;
    installingRef.current = true;

    setPhase('installing');
    setErrorMessage(null);
    setLogLines([]);
    setPercent(0);

    try {
      await invoke('install_openspec_setup');
      setPhase('installed');
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setErrorMessage(message);
      appendLog(message);
      setPhase('error');
    } finally {
      installingRef.current = false;
    }
  }, [appendLog]);

  const skip = useCallback(async () => {
    try {
      await invoke('skip_openspec_setup');
    } catch (error) {
      console.error('[useOpenSpecSetup] Failed to persist skip decision:', error);
    } finally {
      setPhase('skipped');
    }
  }, []);

  return {
    phase,
    logLines,
    percent,
    errorMessage,
    install,
    skip,
    refreshStatus,
    // Whether the dismissible banner/step should be shown at all.
    shouldPrompt: phase === 'idle' || phase === 'installing' || phase === 'error',
    t,
  };
}
