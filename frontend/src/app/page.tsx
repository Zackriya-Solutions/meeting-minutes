'use client';

import { useState, useEffect, useRef } from 'react';
import { useSidebar } from '@/components/Sidebar/SidebarProvider';
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
import { toast } from 'sonner';
import { listen } from '@tauri-apps/api/event';
import { Icon } from '@/components/memento/Icon';
import { useT } from '@/lib/i18n';

export default function Home() {
  // Local page state (not moved to contexts)
  const [isRecording, setIsRecordingState] = useState(false);

  // Use contexts for state management
  const { transcripts } = useTranscripts();
  const { transcriptModelConfig } = useConfig();
  const recordingState = useRecordingState();

  // Extract status from global state
  const { status, isStopping, isProcessing, isSaving } = recordingState;

  // Hooks
  const t = useT();
  const { setIsMeetingActive } = useSidebar();
  const { modals, messages, showModal, hideModal } = useModalState(transcriptModelConfig);
  const { isRecordingDisabled, setIsRecordingDisabled } = useRecordingStateSync(isRecording, setIsRecordingState, setIsMeetingActive);
  const { handleRecordingStart } = useRecordingStart(isRecording, setIsRecordingState, showModal);

  // Get handleRecordingStop function and setIsStopping (state comes from global context)
  const { handleRecordingStop, setIsStopping } = useRecordingStop(
    setIsRecordingState,
    setIsRecordingDisabled
  );

  useEffect(() => {
    let handled = false;
    let pendingStopTimeout: number | undefined;
    const stopAutoListening = () => {
      if (handled || !sessionStorage.getItem('autoStopRecordingSessionId')) return;
      handled = true;
      toast.info(t('Call signal ended — saving the meeting'));
      setIsStopping(true);
      void handleRecordingStop(true);
    };

    window.addEventListener('stop-recording-from-auto-listening', stopAutoListening);
    if (sessionStorage.getItem('autoStopRecordingSessionId')) {
      // Let the recording-state providers finish their initial backend sync after navigation.
      pendingStopTimeout = window.setTimeout(stopAutoListening, 250);
    }
    return () => {
      window.removeEventListener('stop-recording-from-auto-listening', stopAutoListening);
      if (pendingStopTimeout !== undefined) {
        window.clearTimeout(pendingStopTimeout);
      }
    };
  }, [handleRecordingStop, setIsStopping, t]);

  useEffect(() => {
    // Track page view
    Analytics.trackPageView('home');
  }, []);

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

        const unlistenTranscriptionFallback = await listen<string>('transcription-fallback', (event) => {
          toast.warning(t('Cloud transcription unavailable'), {
            description: t(String(event.payload)),
            duration: Infinity,
          });
        });
        unlisteners.push(unlistenTranscriptionFallback);

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
    <div className="memento-shell flex h-screen flex-col">
      {/* All Modals supported*/}
      <SettingsModals
        modals={modals}
        messages={messages}
        onClose={hideModal}
      />

      <div className="flex flex-1 overflow-hidden">
        <TranscriptPanel
          isProcessingStop={isProcessingStop}
          isStopping={isStopping}
          showModal={showModal}
        />

        {/* No in-place recording controls here: <GlobalRecordingPill> owns the
            off-route recording affordance (return + Finish) on every screen. This
            used to render its own RecordOverlay, which was unreachable while the
            navigation guard pinned recording to /recording. With navigation free,
            it would have been a second Finish button backed by a second
            useRecordingStop instance — whose duplicate-stop guard cannot see the
            provider's in-flight stop. */}

        {/* Start-recording button — bottom center of the content area when idle */}
        {!recordingState.isRecording &&
          transcripts.length > 0 &&
          status !== RecordingStatus.STARTING &&
          status !== RecordingStatus.PROCESSING_TRANSCRIPTS &&
          status !== RecordingStatus.SAVING && (
            <div
              className="pointer-events-none fixed bottom-8 right-0 z-10 flex items-center justify-center transition-[left] duration-300"
              style={{ left: 0 }}
            >
              <button
                onClick={() => handleRecordingStart()}
                disabled={isRecordingDisabled}
                aria-label={t('Record meeting')}
                className="memento-primary-action pointer-events-auto inline-flex items-center gap-2 px-6 text-sm font-semibold disabled:cursor-not-allowed disabled:opacity-50"
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
        />
      </div>
    </div>
  );
}
