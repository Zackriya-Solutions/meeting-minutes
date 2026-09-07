import { useState, useEffect, useCallback, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useTranscripts } from '@/contexts/TranscriptContext';
import { useSidebar } from '@/components/Sidebar/SidebarProvider';
import { useConfig } from '@/contexts/ConfigContext';
import { useRecordingState, RecordingStatus } from '@/contexts/RecordingStateContext';
import { recordingService } from '@/services/recordingService';
import Analytics from '@/lib/analytics';
import { showRecordingNotification } from '@/lib/recordingNotification';
import {
  getProviderCommands,
  hasDownloadingModel,
  type ModelWithStatus,
} from '@/lib/transcription-model-readiness';
import { toast } from 'sonner';

interface UseRecordingStartReturn {
  handleRecordingStart: () => Promise<void>;
  isAutoStarting: boolean;
}

/** Provider id for a self-hosted, OpenAI-compatible Whisper server. */
const REMOTE_WHISPER_PROVIDER = 'remoteWhisper';

/** Local engine assumed when the stored config cannot be read. */
const DEFAULT_PROVIDER = 'parakeet';

interface TranscriptConfig {
  provider?: string;
  /** For `remoteWhisper` this carries the server base URL, not a model name. */
  model?: string;
}

interface TranscriptionReadiness {
  ready: boolean;
  reason?:
    | 'downloading'
    | 'missing-local-model'
    | 'remote-unreachable'
    | 'unsupported-provider';
  serverUrl?: string;
  provider?: string;
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

  // Synchronous latch: a rapid double-click re-enters handleRecordingStart
  // before any state update lands, so an async/state guard can't stop it.
  const isStartingRef = useRef(false);

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

  // Read provider and model together: the remote path needs both, and one read
  // keeps the two halves of the decision from disagreeing.
  const getTranscriptConfig = useCallback(async (): Promise<TranscriptConfig> => {
    try {
      const config = await invoke<TranscriptConfig | null>('api_get_transcript_config');
      return {
        provider: config?.provider || DEFAULT_PROVIDER,
        model: config?.model ?? '',
      };
    } catch (error) {
      // Fall back to the local engine: a config read failure must not silently
      // grant access to a backend we could not confirm.
      console.error('Failed to load transcription provider:', error);
      return { provider: DEFAULT_PROVIDER, model: '' };
    }
  }, []);

  /**
   * A remote server owns its own model lifecycle, so "ready" means reachable —
   * there is nothing on disk to check.
   */
  const checkRemoteServerReady = useCallback(
    async (serverUrl: string): Promise<TranscriptionReadiness> => {
      if (!serverUrl) {
        return { ready: false, reason: 'remote-unreachable', serverUrl: '' };
      }

      try {
        const reachable = await invoke<boolean>('remote_whisper_check_health', {
          baseUrl: serverUrl,
        });
        return reachable
          ? { ready: true }
          : { ready: false, reason: 'remote-unreachable', serverUrl };
      } catch (error) {
        console.error('Remote transcription server health check failed:', error);
        return { ready: false, reason: 'remote-unreachable', serverUrl };
      }
    },
    []
  );

  /**
   * Check the *selected* local engine, never a hardcoded one: a Whisper user
   * must not be blocked by a missing Parakeet model, and vice versa.
   */
  const checkLocalModelReady = useCallback(
    async (provider: string): Promise<TranscriptionReadiness> => {
      const commands = getProviderCommands(provider);
      if (!commands) {
        console.error(`Unsupported transcription provider: ${provider}`);
        return { ready: false, reason: 'unsupported-provider', provider };
      }

      try {
        await invoke(commands.initialize);
        if (await invoke<boolean>(commands.hasAvailableModels)) {
          return { ready: true };
        }
      } catch (error) {
        console.error('Failed to check transcription model status:', error);
      }

      try {
        const models = await invoke<ModelWithStatus[]>(commands.getAvailableModels);
        if (hasDownloadingModel(models)) {
          return { ready: false, reason: 'downloading' };
        }
      } catch (error) {
        // Default to "not downloading" so the user gets the model selector
        // rather than a wait-for-it toast that would never resolve.
        console.error('Failed to check model download status:', error);
      }

      return { ready: false, reason: 'missing-local-model' };
    },
    []
  );

  /**
   * Is transcription ready to record?
   *
   * The answer depends on the configured provider: a local engine needs a model
   * on disk, a remote server only needs to be reachable. The Rust recording
   * command validates the same provider again before capture.
   */
  const checkTranscriptionReady = useCallback(async (): Promise<TranscriptionReadiness> => {
    const { provider = DEFAULT_PROVIDER, model = '' } = await getTranscriptConfig();

    if (provider === REMOTE_WHISPER_PROVIDER) {
      // The `model` column carries the server base URL for this provider.
      return checkRemoteServerReady(model.trim());
    }

    return checkLocalModelReady(provider);
  }, [getTranscriptConfig, checkRemoteServerReady, checkLocalModelReady]);

  /**
   * Single place that turns a blocked start into user-facing feedback, so the
   * three entry points (button, auto-start, sidebar) cannot drift apart.
   */
  const reportTranscriptionNotReady = useCallback(
    (readiness: TranscriptionReadiness, source: string) => {
      if (readiness.reason === 'downloading') {
        toast.info('Model download in progress', {
          description: 'Please wait for the transcription model to finish downloading before recording.',
          duration: 5000,
        });
        Analytics.trackButtonClick('start_recording_blocked_downloading', source);
        return;
      }

      if (readiness.reason === 'remote-unreachable') {
        // Downloading a local model would not fix this, so the model selector
        // stays closed — it would point the user at the wrong remedy.
        toast.error('Transcription server unreachable', {
          description: readiness.serverUrl
            ? `Could not reach ${readiness.serverUrl}. Check that the server is running, or switch provider in Settings.`
            : 'No server URL is configured. Set one in Settings > Transcription.',
          duration: 6000,
        });
        Analytics.trackButtonClick('start_recording_blocked_remote_unreachable', source);
        return;
      }

      if (readiness.reason === 'unsupported-provider') {
        // Same reasoning: no local download fixes a provider this build cannot
        // drive, so send the user to Settings instead of the model selector.
        toast.error('Transcription provider not supported', {
          description: `"${readiness.provider}" cannot be used for recording. Pick another provider in Settings > Transcription.`,
          duration: 6000,
        });
        Analytics.trackButtonClick('start_recording_blocked_unsupported', source);
        return;
      }

      toast.error('Transcription model not ready', {
        description: 'Please download a transcription model before recording.',
        duration: 5000,
      });
      showModal?.('modelSelector', 'Transcription model setup required');
      Analytics.trackButtonClick('start_recording_blocked_missing', source);
    },
    [showModal]
  );

  // Handle manual recording start (from button click)
  const handleRecordingStart = useCallback(async () => {
    if (isStartingRef.current) {
      console.log('handleRecordingStart ignored - start already in progress');
      return;
    }
    isStartingRef.current = true;
    try {
      console.log('handleRecordingStart called - checking transcription readiness');

      // Transcription must be ready — local model on disk, or remote server reachable
      const readiness = await checkTranscriptionReady();
      if (!readiness.ready) {
        reportTranscriptionNotReady(readiness, 'home_page');
        setStatus(RecordingStatus.IDLE);
        return;
      }

      console.log('Transcription ready - setting up meeting title and state');

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
      const errorMsg = error instanceof Error ? error.message : String(error);

      // A racing second start that lost to a live recording must not clobber
      // the running recording's state. The winning start is live, so reflect
      // RECORDING here — leaving STARTING latched would keep the Stop button
      // disabled forever, since it's gated on isStartingRecording.
      if (errorMsg.includes('already in progress')) {
        console.warn('Start rejected because recording is already active - leaving live recording state untouched');
        setStatus(RecordingStatus.RECORDING);
        Analytics.trackButtonClick('start_recording_error', 'home_page');
        return;
      }

      if (errorMsg.includes('Recording start timed out')) {
        toast.error('Recording start timed out — please try again');
      }

      setStatus(RecordingStatus.ERROR, error instanceof Error ? error.message : 'Failed to start recording');
      setIsRecording(false); // Reset state on error
      Analytics.trackButtonClick('start_recording_error', 'home_page');
      // Re-throw so RecordingControls can handle device-specific errors
      throw error;
    } finally {
      isStartingRef.current = false;
    }
  }, [generateMeetingTitle, setMeetingTitle, setIsRecording, clearTranscripts, setIsMeetingActive, checkTranscriptionReady, reportTranscriptionNotReady, selectedDevices, showModal, setStatus]);

  // Check for autoStartRecording flag and start recording automatically
  useEffect(() => {
    const checkAutoStartRecording = async () => {
      if (typeof window !== 'undefined') {
        const shouldAutoStart = sessionStorage.getItem('autoStartRecording');
        if (shouldAutoStart === 'true' && !isRecording && !isAutoStarting) {
          console.log('Auto-starting recording from navigation...');
          setIsAutoStarting(true);
          sessionStorage.removeItem('autoStartRecording'); // Clear the flag

          // Transcription must be ready — local model on disk, or remote server reachable
          const readiness = await checkTranscriptionReady();
          if (!readiness.ready) {
            reportTranscriptionNotReady(readiness, 'sidebar_auto');
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
            const errorMsg = error instanceof Error ? error.message : String(error);
            if (errorMsg.includes('already in progress')) {
              // Benign race — another start won and is live; skip ERROR/alert.
              setStatus(RecordingStatus.RECORDING);
              Analytics.trackButtonClick('start_recording_error', 'sidebar_auto');
            } else {
              setStatus(RecordingStatus.ERROR, errorMsg);
              alert('Failed to start recording. Check console for details.');
              Analytics.trackButtonClick('start_recording_error', 'sidebar_auto');
            }
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
    checkTranscriptionReady,
    reportTranscriptionNotReady,
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

      console.log('Direct start from sidebar - checking transcription readiness');
      setIsAutoStarting(true);

      // Transcription must be ready — local model on disk, or remote server reachable
      const readiness = await checkTranscriptionReady();
      if (!readiness.ready) {
        reportTranscriptionNotReady(readiness, 'sidebar_direct');
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
        const errorMsg = error instanceof Error ? error.message : String(error);
        if (errorMsg.includes('already in progress')) {
          // Benign race — another start won and is live; skip ERROR/alert.
          setStatus(RecordingStatus.RECORDING);
          Analytics.trackButtonClick('start_recording_error', 'sidebar_direct');
        } else {
          setStatus(RecordingStatus.ERROR, errorMsg);
          alert('Failed to start recording. Check console for details.');
          Analytics.trackButtonClick('start_recording_error', 'sidebar_direct');
        }
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
    checkTranscriptionReady,
    reportTranscriptionNotReady,
    showModal,
    setStatus,
  ]);

  return {
    handleRecordingStart,
    isAutoStarting,
  };
}
