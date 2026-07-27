import { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useTranscripts } from '@/contexts/TranscriptContext';
import { useSidebar } from '@/components/Sidebar/SidebarProvider';
import { useConfig } from '@/contexts/ConfigContext';
import { useRecordingState, RecordingStatus } from '@/contexts/RecordingStateContext';
import { recordingService } from '@/services/recordingService';
import Analytics from '@/lib/analytics';
import { showRecordingNotification } from '@/lib/recordingNotification';
import { toast } from 'sonner';

interface UseRecordingStartReturn {
  handleRecordingStart: () => Promise<void>;
  isAutoStarting: boolean;
}

/** Analytics call-site label for the three recording entry points. */
type RecordingStartSource = 'home_page' | 'sidebar_auto' | 'sidebar_direct';

/**
 * Custom hook for managing recording start lifecycle.
 * Handles both manual start (button click) and auto-start (from sidebar navigation).
 *
 * Features:
 * - Meeting title generation (format: Meeting DD_MM_YY_HH_MM_SS)
 * - Transcript clearing on start
 * - Analytics tracking
 * - Recording notification display
 * - Auto-start from sidebar via sessionStorage flag
 */
export function useRecordingStart(
  isRecording: boolean,
  setIsRecording: (value: boolean) => void,
  showModal?: (name: 'modelSelector', message?: string) => void
): UseRecordingStartReturn {
  const [isAutoStarting, setIsAutoStarting] = useState(false);

  const { clearTranscripts, setMeetingTitle } = useTranscripts();
  const { setIsMeetingActive } = useSidebar();
  const { selectedDevices } = useConfig();
  const { setStatus } = useRecordingState();

  // Generate meeting title with timestamp
  const generateMeetingTitle = useCallback(() => {
    const now = new Date();
    const day = String(now.getDate()).padStart(2, '0');
    const month = String(now.getMonth() + 1).padStart(2, '0');
    const year = String(now.getFullYear()).slice(-2);
    const hours = String(now.getHours()).padStart(2, '0');
    const minutes = String(now.getMinutes()).padStart(2, '0');
    const seconds = String(now.getSeconds()).padStart(2, '0');
    return `Meeting ${day}_${month}_${year}_${hours}_${minutes}_${seconds}`;
  }, []);

  // Check transcription provider is ready before starting. This is a
  // dispatcher not a Parakeet-only check: each provider has its own
  // "is this usable right now?" rule, and the `disabled` provider
  // skips the check entirely (recording-only mode, no model needed —
  // closes #519 + #338).
  const checkTranscriptProviderReady = useCallback(async (): Promise<{ ok: boolean; reason?: string; provider: string }> => {
    // Pull current provider from saved config via ConfigContext's exposed
    // setter indirectly: the caller already has the provider value bound
    // into the React tree; we re-read it here via the latest invoke.
    try {
      const cfg = await invoke<{ provider: string; model: string } | null>('api_get_transcript_config');
      const provider = cfg?.provider ?? 'parakeet';
      if (provider === 'disabled') {
        return { ok: true, provider };
      }
      if (provider === 'parakeet') {
        await invoke('parakeet_init');
        const hasModels = await invoke<boolean>('parakeet_has_available_models');
        return hasModels
          ? { ok: true, provider }
          : { ok: false, reason: 'parakeet-model-missing', provider };
      }
      if (provider === 'localWhisper') {
        // Whisper is loaded lazily on first transcribe; just confirm we have
        // any Whisper models on disk via the engine command, not by booting
        // a model up-front (we don't want to pay model-load latency on every
        // Record-click).
        try {
          await invoke<unknown>('whisper_init');
          const models = await invoke<Array<{ name: string; status: unknown }>>('whisper_get_available_models');
          const available = (models ?? []).some((m) => {
            const s = (m && (m as { status?: unknown }).status) as unknown;
            if (typeof s === 'string') return s === 'Available' || s === 'available';
            if (s && typeof s === 'object') return Object.prototype.hasOwnProperty.call(s, 'Available');
            return false;
          });
          return available
            ? { ok: true, provider }
            : { ok: false, reason: 'whisper-model-missing', provider };
        } catch {
          return { ok: false, reason: 'whisper-init-failed', provider };
        }
      }
      if (provider === 'remote') {
        try {
          await invoke<unknown>('api_get_transcript_remote_config');
          return { ok: true, provider };
        } catch {
          return { ok: false, reason: 'remote-config-missing', provider };
        }
      }
      if (provider === 'groq' || provider === 'deepgram' || provider === 'elevenLabs' || provider === 'openai') {
        try {
          const key = await invoke<string>('api_get_transcript_api_key', { provider });
          return key && key.length > 0
            ? { ok: true, provider }
            : { ok: false, reason: `${provider}-api-key-missing`, provider };
        } catch {
          return { ok: false, reason: `${provider}-api-key-missing`, provider };
        }
      }
      // Unknown provider — fall back to the historically-working Parakeet check.
      await invoke('parakeet_init');
      const hasModels = await invoke<boolean>('parakeet_has_available_models');
      return hasModels ? { ok: true, provider } : { ok: false, reason: 'parakeet-model-missing', provider };
    } catch (error) {
      console.error('Failed to check transcription provider status:', error);
      return { ok: false, reason: 'unexpected', provider: 'unknown' };
    }
  }, []);

  // Check if any model is currently downloading (Parakeet only — Whisper
  // doesn't expose a download-in-progress signal here, so we cover the
  // parakeet side and accept that Whisper will fail its own check).
  const checkIfModelDownloading = useCallback(async (): Promise<boolean> => {
    try {
      const models = await invoke<any[]>('parakeet_get_available_models');
      const isDownloading = models.some(m =>
        m.status && (
          typeof m.status === 'object'
            ? 'Downloading' in m.status
            : m.status === 'Downloading'
        )
      );
      return isDownloading;
    } catch (error) {
      console.error('Failed to check model download status:', error);
      return false;
    }
  }, []);

  /** Shared handler for a failed transcription-provider readiness check.
   *  Shows a toast with a context-sensitive message based on `tr.reason`,
   *  optionally triggers the model-selector modal, and resets status to IDLE.
   *  `source` is the analytics call-site label, preserved per entry point so
   *  the shared guard doesn't collapse the three start paths into one bucket.
   *  Returns `true` if the guard blocked recording (caller should return early). */
  const guardTranscriptionModel = useCallback(async (
    tr: { ok: boolean; reason?: string; provider: string },
    source: RecordingStartSource,
    showModal?: (name: 'modelSelector', message?: string) => void,
  ): Promise<boolean> => {
    if (tr.ok) return false;

    const isDownloading = await checkIfModelDownloading();
    if (isDownloading && tr.provider === 'parakeet') {
      toast.info('Model download in progress', {
        description: 'Please wait for the transcription model to finish downloading before recording.',
        duration: 5000,
      });
      Analytics.trackButtonClick('start_recording_blocked_downloading', source);
    } else {
      const reasonText =
        tr.reason === 'whisper-model-missing' ? 'Whisper model not downloaded — open Settings → Transcription to download one'
        : tr.reason === 'parakeet-model-missing' ? 'Parakeet model not downloaded — open Settings → Transcription to download one'
        : tr.reason === 'remote-config-missing' ? 'Remote provider has no endpoint configured — open Settings → Transcription'
        : tr.reason && tr.reason.endsWith('-api-key-missing') ? 'API key missing — open Settings → Transcription to paste it'
        : tr.reason === 'whisper-init-failed' ? 'Failed to initialize Whisper engine'
        : 'Transcription model not ready';
      toast.error('Transcription model not ready', {
        description: reasonText,
        duration: 5000,
      });
      showModal?.('modelSelector', 'Transcription model setup required');
      Analytics.trackButtonClick('start_recording_blocked_missing', source);
    }
    setStatus(RecordingStatus.IDLE);
    return true;
  }, [checkIfModelDownloading, setStatus]);

  // Handle manual recording start (from button click)
  const handleRecordingStart = useCallback(async () => {
    try {
      console.log('handleRecordingStart called - checking transcription provider status');

      // Check if transcription provider is ready before starting
      const tr = await checkTranscriptProviderReady();
      if (await guardTranscriptionModel(tr, 'home_page', showModal)) return;

      console.log('Provider ready - setting up meeting title and state');

      const randomTitle = generateMeetingTitle();
      setMeetingTitle(randomTitle);

      // Set STARTING status before initiating backend recording
      setStatus(RecordingStatus.STARTING, 'Initializing recording...');

      // Start the actual backend recording
      console.log('Starting backend recording with meeting:', randomTitle);
      await recordingService.startRecordingWithDevices(
        selectedDevices?.micDevice || null,
        selectedDevices?.systemDevice || null,
        randomTitle
      );
      console.log('Backend recording started successfully');

      // Update state after successful backend start
      // Note: RECORDING status will be set by RecordingStateContext event listener
      console.log('Setting isRecordingState to true');
      setIsRecording(true); // This will also update the sidebar via the useEffect
      clearTranscripts(); // Clear previous transcripts when starting new recording
      setIsMeetingActive(true);
      Analytics.trackButtonClick('start_recording', 'home_page');

      // Show recording notification if enabled
      await showRecordingNotification();
    } catch (error) {
      console.error('Failed to start recording:', error);
      setStatus(RecordingStatus.ERROR, error instanceof Error ? error.message : 'Failed to start recording');
      setIsRecording(false); // Reset state on error
      Analytics.trackButtonClick('start_recording_error', 'home_page');
      // Re-throw so RecordingControls can handle device-specific errors
      throw error;
    }
  }, [generateMeetingTitle, setMeetingTitle, setIsRecording, clearTranscripts, setIsMeetingActive, checkTranscriptProviderReady, guardTranscriptionModel, selectedDevices, showModal, setStatus]);

  // Check for autoStartRecording flag and start recording automatically
  useEffect(() => {
    const checkAutoStartRecording = async () => {
      if (typeof window !== 'undefined') {
        const shouldAutoStart = sessionStorage.getItem('autoStartRecording');
        if (shouldAutoStart === 'true' && !isRecording && !isAutoStarting) {
          console.log('Auto-starting recording from navigation...');
          setIsAutoStarting(true);
          sessionStorage.removeItem('autoStartRecording'); // Clear the flag

          // Check if transcription provider is ready before starting
          const tr = await checkTranscriptProviderReady();
          if (await guardTranscriptionModel(tr, 'sidebar_auto', showModal)) {
            setIsAutoStarting(false);
            return;
          }

          // Start the actual backend recording
          try {
            // Generate meeting title
            const generatedMeetingTitle = generateMeetingTitle();

            // Set STARTING status before initiating backend recording
            setStatus(RecordingStatus.STARTING, 'Initializing recording...');

            console.log('Auto-starting backend recording with meeting:', generatedMeetingTitle);
            const result = await recordingService.startRecordingWithDevices(
              selectedDevices?.micDevice || null,
              selectedDevices?.systemDevice || null,
              generatedMeetingTitle
            );
            console.log('Auto-start backend recording result:', result);

            // Update UI state after successful backend start
            // Note: RECORDING status will be set by RecordingStateContext event listener
            setMeetingTitle(generatedMeetingTitle);
            setIsRecording(true);
            clearTranscripts();
            setIsMeetingActive(true);
            Analytics.trackButtonClick('start_recording', 'sidebar_auto');

            // Show recording notification if enabled
            await showRecordingNotification();
          } catch (error) {
            console.error('Failed to auto-start recording:', error);
            setStatus(RecordingStatus.ERROR, error instanceof Error ? error.message : 'Failed to auto-start recording');
            alert('Failed to start recording. Check console for details.');
            Analytics.trackButtonClick('start_recording_error', 'sidebar_auto');
          } finally {
            setIsAutoStarting(false);
          }
        }
      }
    };

    checkAutoStartRecording();
  }, [
    isRecording,
    isAutoStarting,
    selectedDevices,
    generateMeetingTitle,
    setMeetingTitle,
    setIsRecording,
    clearTranscripts,
    setIsMeetingActive,
    checkTranscriptProviderReady,
    guardTranscriptionModel,
    showModal,
    setStatus,
  ]);

  // Listen for direct recording trigger from sidebar when already on home page
  useEffect(() => {
    const handleDirectStart = async () => {
      if (isRecording || isAutoStarting) {
        console.log('Recording already in progress, ignoring direct start event');
        return;
      }

      console.log('Direct start from sidebar - checking transcription provider status');
      setIsAutoStarting(true);

      // Check if transcription provider is ready before starting
      const tr = await checkTranscriptProviderReady();
      if (await guardTranscriptionModel(tr, 'sidebar_direct', showModal)) {
        setIsAutoStarting(false);
        return;
      }

      try {
        // Generate meeting title
        const generatedMeetingTitle = generateMeetingTitle();

        // Set STARTING status before initiating backend recording
        setStatus(RecordingStatus.STARTING, 'Initializing recording...');

        console.log('Starting backend recording with meeting:', generatedMeetingTitle);
        const result = await recordingService.startRecordingWithDevices(
          selectedDevices?.micDevice || null,
          selectedDevices?.systemDevice || null,
          generatedMeetingTitle
        );
        console.log('Backend recording result:', result);

        // Update UI state after successful backend start
        // Note: RECORDING status will be set by RecordingStateContext event listener
        setMeetingTitle(generatedMeetingTitle);
        setIsRecording(true);
        clearTranscripts();
        setIsMeetingActive(true);
        Analytics.trackButtonClick('start_recording', 'sidebar_direct');

        // Show recording notification if enabled
        await showRecordingNotification();
      } catch (error) {
        console.error('Failed to start recording from sidebar:', error);
        setStatus(RecordingStatus.ERROR, error instanceof Error ? error.message : 'Failed to start recording from sidebar');
        alert('Failed to start recording. Check console for details.');
        Analytics.trackButtonClick('start_recording_error', 'sidebar_direct');
      } finally {
        setIsAutoStarting(false);
      }
    };

    window.addEventListener('start-recording-from-sidebar', handleDirectStart);

    return () => {
      window.removeEventListener('start-recording-from-sidebar', handleDirectStart);
    };
  }, [
    isRecording,
    isAutoStarting,
    selectedDevices,
    generateMeetingTitle,
    setMeetingTitle,
    setIsRecording,
    clearTranscripts,
    setIsMeetingActive,
    checkTranscriptProviderReady,
    guardTranscriptionModel,
    showModal,
    setStatus,
  ]);

  return {
    handleRecordingStart,
    isAutoStarting,
  };
}
