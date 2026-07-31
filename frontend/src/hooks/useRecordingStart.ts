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
  const { selectedDevices, transcriptModelConfig } = useConfig();
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

  // Check if the selected transcription model is ready
  const checkModelReady = useCallback(async (): Promise<boolean> => {
    try {
      if (transcriptModelConfig.provider === 'whisper') {
        return await invoke<boolean>('whisper_has_available_models');
      } else {
        await invoke('parakeet_init');
        return await invoke<boolean>('parakeet_has_available_models');
      }
    } catch (error) {
      console.error(`Failed to check ${transcriptModelConfig.provider} status:`, error);
      return false;
    }
  }, [transcriptModelConfig.provider]);

  // Check if any model for the current provider is downloading
  const checkIfModelDownloading = useCallback(async (): Promise<boolean> => {
    try {
      const command = transcriptModelConfig.provider === 'whisper'
        ? 'whisper_get_available_models'
        : 'parakeet_get_available_models';

      const models = await invoke<any[]>(command);
      const isDownloading = models.some(m =>
        m.status && (
          typeof m.status === 'object'
            ? 'Downloading' in m.status
            : m.status === 'Downloading'
        )
      );
      return isDownloading;
    } catch (error) {
      console.error(`Failed to check ${transcriptModelConfig.provider} download status:`, error);
      return false; // Default to not downloading (will show error + modal)
    }
  }, [transcriptModelConfig.provider]);

  // Handle manual recording start (from button click)
  const handleRecordingStart = useCallback(async () => {
    try {
      console.log('handleRecordingStart called - checking model status');

      // Check if transcription model is ready before starting
      const isModelReady = await checkModelReady();
      if (!isModelReady) {
        const isDownloading = await checkIfModelDownloading();
        if (isDownloading) {
          toast.info('Model download in progress', {
            description: 'Please wait for the transcription model to finish downloading before recording.',
            duration: 5000,
          });
          Analytics.trackButtonClick('start_recording_blocked_downloading', 'home_page');
        } else {
          toast.error('Transcription model not ready', {
            description: 'Please download a transcription model before recording.',
            duration: 5000,
          });
          showModal?.('modelSelector', 'Transcription model setup required');
          Analytics.trackButtonClick('start_recording_blocked_missing', 'home_page');
        }
        setStatus(RecordingStatus.IDLE);
        return;
      }

      console.log('Model ready - setting up meeting title and state');

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
  }, [generateMeetingTitle, setMeetingTitle, setIsRecording, clearTranscripts, setIsMeetingActive, checkModelReady, checkIfModelDownloading, selectedDevices, showModal, setStatus]);

  // Check for autoStartRecording flag and start recording automatically
  useEffect(() => {
    const checkAutoStartRecording = async () => {
      if (typeof window !== 'undefined') {
        const shouldAutoStart = sessionStorage.getItem('autoStartRecording');
        if (shouldAutoStart === 'true' && !isRecording && !isAutoStarting) {
          console.log('Auto-starting recording from navigation...');
          setIsAutoStarting(true);
          sessionStorage.removeItem('autoStartRecording'); // Clear the flag

          // Check if transcription model is ready before starting
          const isModelReady = await checkModelReady();
          if (!isModelReady) {
            const isDownloading = await checkIfModelDownloading();
            if (isDownloading) {
              toast.info('Model download in progress', {
                description: 'Please wait for the transcription model to finish downloading before recording.',
                duration: 5000,
              });
              Analytics.trackButtonClick('start_recording_blocked_downloading', 'sidebar_auto');
            } else {
              toast.error('Transcription model not ready', {
                description: 'Please download a transcription model before recording.',
                duration: 5000,
              });
              showModal?.('modelSelector', 'Transcription model setup required');
              Analytics.trackButtonClick('start_recording_blocked_missing', 'sidebar_auto');
            }
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
    checkModelReady,
    checkIfModelDownloading,
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

      console.log('Direct start from sidebar - checking model status');
      setIsAutoStarting(true);

      // Check if transcription model is ready before starting
      const isModelReady = await checkModelReady();
      if (!isModelReady) {
        const isDownloading = await checkIfModelDownloading();
        if (isDownloading) {
          toast.info('Model download in progress', {
            description: 'Please wait for the transcription model to finish downloading before recording.',
            duration: 5000,
          });
          Analytics.trackButtonClick('start_recording_blocked_downloading', 'sidebar_direct');
        } else {
          toast.error('Transcription model not ready', {
            description: 'Please download a transcription model before recording.',
            duration: 5000,
          });
          showModal?.('modelSelector', 'Transcription model setup required');
          Analytics.trackButtonClick('start_recording_blocked_missing', 'sidebar_direct');
        }
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
    checkModelReady,
    checkIfModelDownloading,
    showModal,
    setStatus,
  ]);

  return {
    handleRecordingStart,
    isAutoStarting,
  };
}
