'use client';

import { useState, useEffect, useRef } from 'react';
import { motion } from 'framer-motion';
import { useSidebar } from '@/components/Sidebar/SidebarProvider';
import { usePermissionCheck } from '@/hooks/usePermissionCheck';
import { useRecordingState, RecordingStatus } from '@/contexts/RecordingStateContext';
import { useTranscripts } from '@/contexts/TranscriptContext';
import { useConfig } from '@/contexts/ConfigContext';
import { StatusOverlays } from '@/app/_components/StatusOverlays';
import Analytics from '@/lib/analytics';
import { SettingsModals } from './_components/SettingsModal';
import { TranscriptPanel } from './_components/TranscriptPanel';
import { useModalState } from '@/hooks/useModalState';
import { useRecordingStateSync } from '@/hooks/useRecordingStateSync';
import { useRecordingStart } from '@/hooks/useRecordingStart';
import { useRecordingStop } from '@/hooks/useRecordingStop';
import { useTranscriptRecovery } from '@/hooks/useTranscriptRecovery';
import { TranscriptRecovery } from '@/components/TranscriptRecovery';
import { indexedDBService } from '@/services/indexedDBService';
import { toast } from 'sonner';
import { useRouter } from 'next/navigation';
import { listen } from '@tauri-apps/api/event';
import { RecordOverlay } from '@/components/memento/RecordOverlay';
import { Icon } from '@/components/memento/Icon';
import { useT } from '@/lib/i18n';

export default function Home() {
  // Local page state (not moved to contexts)
  const [isRecording, setIsRecordingState] = useState(false);
  const [showRecoveryDialog, setShowRecoveryDialog] = useState(false);

  // Use contexts for state management
  const { meetingTitle, currentMeetingId } = useTranscripts();
  const { transcriptModelConfig, selectedDevices } = useConfig();
  const recordingState = useRecordingState();

  // Extract status from global state
  const { status, isStopping, isProcessing, isSaving } = recordingState;

  // Hooks
  const t = useT();
  const { hasMicrophone } = usePermissionCheck();
  const { setIsMeetingActive, isCollapsed: sidebarCollapsed, sidebarWidth, refetchMeetings } = useSidebar();
  const { modals, messages, showModal, hideModal } = useModalState(transcriptModelConfig);
  const { isRecordingDisabled, setIsRecordingDisabled } = useRecordingStateSync(isRecording, setIsRecordingState, setIsMeetingActive);
  const { handleRecordingStart } = useRecordingStart(isRecording, setIsRecordingState, showModal);

  // Get handleRecordingStop function and setIsStopping (state comes from global context)
  const { handleRecordingStop, setIsStopping } = useRecordingStop(
    setIsRecordingState,
    setIsRecordingDisabled
  );

  // Recovery hook
  const {
    recoverableMeetings,
    isLoading: isLoadingRecovery,
    isRecovering,
    checkForRecoverableTranscripts,
    recoverMeeting,
    loadMeetingTranscripts,
    deleteRecoverableMeeting
  } = useTranscriptRecovery();

  const router = useRouter();

  useEffect(() => {
    // Track page view
    Analytics.trackPageView('home');
  }, []);

  // Startup recovery check
  useEffect(() => {
    const performStartupChecks = async () => {
      try {
        // Skip recovery check if currently recording or processing stop
        // This prevents the recovery dialog from showing when:
        if (recordingState.isRecording ||
          status === RecordingStatus.STOPPING ||
          status === RecordingStatus.PROCESSING_TRANSCRIPTS ||
          status === RecordingStatus.SAVING) {
          console.log('Skipping recovery check - recording in progress or processing');
          return;
        }

        // 1. Clean up old meetings (7+ days)
        try {
          await indexedDBService.deleteOldMeetings(7);
        } catch (error) {
          console.warn('⚠️ Failed to clean up old meetings:', error);
        }

        // 2. Clean up saved meetings (24+ hours after save)
        try {
          await indexedDBService.deleteSavedMeetings(24);
        } catch (error) {
          console.warn('⚠️ Failed to clean up saved meetings:', error);
        }

        // 3. Always check for recoverable meetings on startup
        // Don't skip based on sessionStorage - we need to check every time
        await checkForRecoverableTranscripts();
      } catch (error) {
        console.error('Failed to perform startup checks:', error);
      }
    };

    performStartupChecks();
  }, [checkForRecoverableTranscripts, recordingState.isRecording, status]);

  // Watch for recoverable meetings changes and show dialog once per session
  useEffect(() => {
    // Only show dialog if we have meetings and haven't shown it yet this session
    if (recoverableMeetings.length > 0) {
      const shownThisSession = sessionStorage.getItem('recovery_dialog_shown');
      if (!shownThisSession) {
        setShowRecoveryDialog(true);
        sessionStorage.setItem('recovery_dialog_shown', 'true');
      }
    }
  }, [recoverableMeetings]);

  // Handle recovery with toast notifications and navigation
  const handleRecovery = async (meetingId: string) => {
    try {
      const result = await recoverMeeting(meetingId);

      if (result.success) {
        toast.success(t('Meeting recovered successfully!'), {
          description: result.audioRecoveryStatus?.status === 'success'
            ? t('Transcripts and audio recovered')
            : t('Transcripts recovered (no audio available)'),
          action: result.meetingId ? {
            label: t('View Meeting'),
            onClick: () => {
              router.push(`/meeting-details?id=${result.meetingId}`);
            }
          } : undefined,
          duration: 10000,
        });

        // Refresh sidebar to show the newly recovered meeting
        await refetchMeetings();

        // If no more recoverable meetings, clear session flag so dialog can show again
        if (recoverableMeetings.length === 0) {
          sessionStorage.removeItem('recovery_dialog_shown');
        }

        // Auto-navigate after a short delay
        if (result.meetingId) {
          setTimeout(() => {
            router.push(`/meeting-details?id=${result.meetingId}`);
          }, 2000);
        }
      }
    } catch (error) {
      toast.error(t('Failed to recover meeting'), {
        description: error instanceof Error ? error.message : t('Unknown error occurred'),
      });
      throw error;
    }
  };

  // Handle dialog close - clear session flag if no meetings left
  const handleDialogClose = () => {
    setShowRecoveryDialog(false);
    // If user closes dialog and there are no more meetings, clear the flag
    // This allows the dialog to show again next session if new meetings appear
    if (recoverableMeetings.length === 0) {
      sessionStorage.removeItem('recovery_dialog_shown');
    }
  };

  // Recording error handling.
  // These listeners previously lived inside <RecordingControls>, which is no
  // longer rendered. They must stay registered so a fatal transcription error
  // still tears the recording down (parity with the old behavior). The
  // structured `transcription-error` event is additionally surfaced to the user
  // (toast / model selector) by useModalState, so here we only stop recording;
  // the plain `transcript-error` string event is surfaced via the error alert.
  // Per-chunk transcription failures (e.g. a cloud provider erroring mid-meeting)
  // only emit 'transcription-warning' from the Rust worker — the recording keeps
  // going but transcripts silently stop. Surface them (throttled: a failing
  // provider warns on every chunk) so the user sees WHY the transcript stalled.
  const lastTranscriptionWarningAtRef = useRef(0);

  useEffect(() => {
    const unlisteners: Array<() => void> = [];

    const setupListeners = async () => {
      try {
        const unlistenTranscriptionWarning = await listen<string>('transcription-warning', (event) => {
          const message = String(event.payload);
          console.warn('transcription-warning received:', message);
          const now = Date.now();
          if (now - lastTranscriptionWarningAtRef.current > 15000) {
            lastTranscriptionWarningAtRef.current = now;
            toast.warning(t('Transcription issue'), { description: message, duration: 8000 });
          }
        });
        unlisteners.push(unlistenTranscriptionWarning);

        const unlistenTranscriptError = await listen<string>('transcript-error', (event) => {
          const errorMessage = String(event.payload);
          console.error('transcript-error received:', errorMessage);
          Analytics.trackTranscriptionError(errorMessage);
          handleRecordingStop(false);
          showModal('errorAlert', errorMessage);
        });
        unlisteners.push(unlistenTranscriptError);

        const unlistenTranscriptionError = await listen('transcription-error', (event) => {
          const payload = event.payload;
          const errorMessage =
            typeof payload === 'object' && payload !== null
              ? ((payload as { userMessage?: string; error?: string }).userMessage ??
                 (payload as { error?: string }).error ??
                 'Transcription error')
              : String(payload);
          console.error('transcription-error received:', errorMessage);
          Analytics.trackTranscriptionError(errorMessage);
          handleRecordingStop(false);
        });
        unlisteners.push(unlistenTranscriptionError);
      } catch (error) {
        console.error('Failed to set up recording error listeners:', error);
      }
    };

    setupListeners();

    return () => {
      unlisteners.forEach((unlisten) => unlisten && unlisten());
    };
  }, [handleRecordingStop, showModal, t]);

  // Computed values using global status
  const isProcessingStop = status === RecordingStatus.PROCESSING_TRANSCRIPTS || isProcessing;

  return (
    <motion.div
      initial={{ opacity: 0, y: 20 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.3, ease: 'easeOut' }}
      className="memento-shell flex flex-col h-screen"
    >
      {/* All Modals supported*/}
      <SettingsModals
        modals={modals}
        messages={messages}
        onClose={hideModal}
      />

      {/* Recovery Dialog */}
      <TranscriptRecovery
        isOpen={showRecoveryDialog}
        onClose={handleDialogClose}
        recoverableMeetings={recoverableMeetings}
        onRecover={handleRecovery}
        onDelete={deleteRecoverableMeeting}
        onLoadPreview={loadMeetingTranscripts}
      />
      <div className="flex flex-1 overflow-hidden">
        <TranscriptPanel
          isProcessingStop={isProcessingStop}
          isStopping={isStopping}
          showModal={showModal}
        />

        {/* Recording controls - only show when permissions are granted or already recording and not showing status messages */}
        {recordingState.isRecording &&
          status !== RecordingStatus.PROCESSING_TRANSCRIPTS &&
          status !== RecordingStatus.SAVING && (
            <div className="fixed bottom-6 right-6 z-10">
              <RecordOverlay
                title={meetingTitle || t('New meeting')}
                meetingId={currentMeetingId}
                onStop={() => {
                  setIsStopping(true);
                  handleRecordingStop(true);
                }}
              />
            </div>
          )}

        {/* Start-recording button — bottom center of the content area when idle */}
        {!recordingState.isRecording &&
          status !== RecordingStatus.STARTING &&
          status !== RecordingStatus.PROCESSING_TRANSCRIPTS &&
          status !== RecordingStatus.SAVING && (
            <div
              className="pointer-events-none fixed bottom-8 right-0 z-10 flex justify-center transition-[left] duration-300"
              style={{ left: sidebarCollapsed ? '4rem' : `${sidebarWidth}px` }}
            >
              <button
                onClick={() => handleRecordingStart()}
                disabled={isRecordingDisabled}
                aria-label={t('Record meeting')}
                className="memento-primary-action mm-press pointer-events-auto inline-flex items-center gap-2 px-6 text-sm font-semibold disabled:cursor-not-allowed disabled:opacity-50"
              >
                <Icon name="mic" size={18} />
                {t('Record meeting')}
              </button>
            </div>
          )}

        {/* Status Overlays - Processing and Saving */}
        <StatusOverlays
          isProcessing={status === RecordingStatus.PROCESSING_TRANSCRIPTS && !recordingState.isRecording}
          isSaving={status === RecordingStatus.SAVING}
          sidebarCollapsed={sidebarCollapsed}
        />
      </div>
    </motion.div>
  );
}
