import { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useTranscripts } from '@/contexts/TranscriptContext';
import { useSidebar } from '@/components/Sidebar/SidebarProvider';
import { useConfig } from '@/contexts/ConfigContext';
import { useRecordingState, RecordingStatus } from '@/contexts/RecordingStateContext';
import { recordingService } from '@/services/recordingService';
import Analytics from '@/lib/analytics';
import { showRecordingNotification } from '@/lib/recordingNotification';
import {
  getConfiguredTranscriptionReadiness,
  type TranscriptionReadiness,
} from '@/lib/transcriptionReadiness';
import { toast } from 'sonner';
import { useT } from '@/lib/i18n';

interface UseRecordingStartReturn {
  handleRecordingStart: () => Promise<void>;
  isAutoStarting: boolean;
}

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
  const t = useT();
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
    const prefix = typeof window !== 'undefined' && sessionStorage.getItem('autoListeningStart') === 'true'
      ? 'Auto meeting'
      : 'Meeting';
    return `${prefix} ${day}_${month}_${year}_${hours}_${minutes}_${seconds}`;
  }, []);

  const reportAutoListeningFailure = useCallback(async (
    failureReason: 'model_unavailable' | 'permission_denied' | 'start_failed',
  ) => {
    const sessionId = sessionStorage.getItem('autoListeningSessionId');
    if (!sessionId || sessionStorage.getItem('autoListeningStart') !== 'true') return;
    try {
      await invoke('report_auto_listening_start', {
        input: { session_id: sessionId, success: false, failure_reason: failureReason },
      });
    } catch (error) {
      console.warn('Failed to persist auto-listening failure:', error);
    }
    sessionStorage.removeItem('autoListeningSessionId');
    sessionStorage.removeItem('autoListeningStartReported');
    sessionStorage.removeItem('autoListeningStart');
  }, []);

  // Check whether the CONFIGURED transcription model is ready (provider-aware).
  // Delegates to the unit-tested helper; name kept for call-site stability.
  const checkTranscriptionReadiness = useCallback(async (): Promise<TranscriptionReadiness> => {
    try {
      return await getConfiguredTranscriptionReadiness();
    } catch (error) {
      console.error('Failed to check transcription model status:', error);
      return { provider: 'unknown', ready: false, requiresLocalModel: true };
    }
  }, []);

  // Check if any model is currently downloading
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
      return false; // Default to not downloading (will show error + modal)
    }
  }, []);

  const showTranscriptionReadinessError = useCallback(async (
    readiness: TranscriptionReadiness,
    analyticsLocation: string,
  ) => {
    if (!readiness.requiresLocalModel) {
      toast.error(t('Managed transcription is unavailable'), {
        description: t('SaluteSpeech does not need a local model. Check internet access and the managed gateway, or switch to GigaAM in Settings → Transcription.'),
        duration: 7000,
      });
      Analytics.trackButtonClick('start_recording_blocked_managed_unavailable', analyticsLocation);
      return;
    }

    const isDownloading = await checkIfModelDownloading();
    if (isDownloading) {
      toast.info(t('Model download in progress'), {
        description: t('Please wait for the transcription model to finish downloading before recording.'),
        duration: 5000,
      });
      Analytics.trackButtonClick('start_recording_blocked_downloading', analyticsLocation);
    } else {
      toast.error(t('Transcription model not ready'), {
        description: t('Please download a transcription model before recording.'),
        duration: 5000,
      });
      showModal?.('modelSelector', t('Transcription model setup required'));
      Analytics.trackButtonClick('start_recording_blocked_missing', analyticsLocation);
    }
  }, [checkIfModelDownloading, showModal, t]);

  // Handle manual recording start (from button click)
  const handleRecordingStart = useCallback(async () => {
    try {
      console.log('handleRecordingStart called - checking Parakeet model status');

      // Check if Parakeet transcription model is ready before starting
      const readiness = await checkTranscriptionReadiness();
      if (!readiness.ready) {
        await showTranscriptionReadinessError(readiness, 'home_page');
        setStatus(RecordingStatus.IDLE);
        return;
      }

      console.log('Parakeet ready - setting up meeting title and state');

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
  }, [generateMeetingTitle, setMeetingTitle, setIsRecording, clearTranscripts, setIsMeetingActive, checkTranscriptionReadiness, showTranscriptionReadinessError, selectedDevices, setStatus]);

  // Check for autoStartRecording flag and start recording automatically
  useEffect(() => {
    const checkAutoStartRecording = async () => {
      if (typeof window !== 'undefined') {
        const shouldAutoStart = sessionStorage.getItem('autoStartRecording');
        if (shouldAutoStart === 'true' && !isRecording && !isAutoStarting) {
          console.log('Auto-starting recording from navigation...');
          setIsAutoStarting(true);
          sessionStorage.removeItem('autoStartRecording'); // Clear the flag

          // Check if Parakeet transcription model is ready before starting
          const readiness = await checkTranscriptionReadiness();
          if (!readiness.ready) {
            await showTranscriptionReadinessError(readiness, 'sidebar_auto');
            await reportAutoListeningFailure('model_unavailable');
            setStatus(RecordingStatus.IDLE);
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
            sessionStorage.removeItem('autoListeningStart');
          } catch (error) {
            console.error('Failed to auto-start recording:', error);
            setStatus(RecordingStatus.ERROR, error instanceof Error ? error.message : 'Failed to auto-start recording');
            alert(t('Failed to start recording. Check console for details.'));
            Analytics.trackButtonClick('start_recording_error', 'sidebar_auto');
            await reportAutoListeningFailure('start_failed');
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
    checkTranscriptionReadiness,
    showTranscriptionReadinessError,
    setStatus,
    reportAutoListeningFailure,
  ]);

  // Listen for direct recording trigger from sidebar when already on home page
  useEffect(() => {
    const handleDirectStart = async () => {
      if (isRecording || isAutoStarting) {
        console.log('Recording already in progress, ignoring direct start event');
        return;
      }

      console.log('Direct start from sidebar - checking Parakeet model status');
      setIsAutoStarting(true);

      // Check if Parakeet transcription model is ready before starting
      const readiness = await checkTranscriptionReadiness();
      if (!readiness.ready) {
        await showTranscriptionReadinessError(readiness, 'sidebar_direct');
        await reportAutoListeningFailure('model_unavailable');
        setStatus(RecordingStatus.IDLE);
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
        sessionStorage.removeItem('autoListeningStart');
      } catch (error) {
        console.error('Failed to start recording from sidebar:', error);
        setStatus(RecordingStatus.ERROR, error instanceof Error ? error.message : 'Failed to start recording from sidebar');
        alert(t('Failed to start recording. Check console for details.'));
        Analytics.trackButtonClick('start_recording_error', 'sidebar_direct');
        await reportAutoListeningFailure('start_failed');
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
    checkTranscriptionReadiness,
    showTranscriptionReadinessError,
    setStatus,
    reportAutoListeningFailure,
  ]);

  return {
    handleRecordingStart,
    isAutoStarting,
  };
}
