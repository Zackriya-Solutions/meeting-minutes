import { VirtualizedTranscriptView } from '@/components/VirtualizedTranscriptView';
import { PermissionWarning } from '@/components/PermissionWarning';
import { Button } from '@/components/ui/button';
import { ButtonGroup } from '@/components/ui/button-group';
import { Copy, GlobeIcon } from 'lucide-react';
import { useTranscripts } from '@/contexts/TranscriptContext';
import { useConfig } from '@/contexts/ConfigContext';
import { useRecordingState } from '@/contexts/RecordingStateContext';
import { usePermissionCheck } from '@/hooks/usePermissionCheck';
import { ModalType } from '@/hooks/useModalState';
import { useIsLinux } from '@/hooks/usePlatform';
import { useMemo } from 'react';

/**
 * TranscriptPanel Component
 *
 * Displays transcript content with controls for copying and language settings.
 * Uses TranscriptContext, ConfigContext, and RecordingStateContext internally.
 */

interface TranscriptPanelProps {
  // indicates stop-processing state for transcripts; derived from backend statuses.
  isProcessingStop: boolean;
  isStopping: boolean;
  showModal: (name: ModalType, message?: string) => void;
}

export function TranscriptPanel({
  isProcessingStop,
  isStopping,
  showModal
}: TranscriptPanelProps) {
  // Contexts
  const { transcripts, transcriptContainerRef, copyTranscript, meetingTitle } = useTranscripts();
  const { transcriptModelConfig } = useConfig();
  const { isRecording, isPaused } = useRecordingState();
  const { checkPermissions, isChecking, hasSystemAudio, hasMicrophone } = usePermissionCheck();
  const isLinux = useIsLinux();

  // Convert transcripts to segments for virtualized view
  const segments = useMemo(() =>
    transcripts.map(t => ({
      id: t.id,
      timestamp: t.audio_start_time ?? 0,
      endTime: t.audio_end_time,
      text: t.text,
      confidence: t.confidence,
    })),
    [transcripts]
  );

  return (
    <div ref={transcriptContainerRef} className="w-full h-full bg-[var(--pt-bg)] flex flex-col overflow-y-auto custom-scrollbar">
      {/* Title area - Sticky header */}
      <div className="sticky top-0 z-10 bg-[rgba(247,246,242,.96)] px-6 md:px-8 py-5 border-b border-[var(--pt-border)]">
        <div className="max-w-[1180px] mx-auto flex items-center justify-between gap-5">
          <div className="min-w-0">
            <p className="pt-label text-[var(--pt-text-tertiary)] mb-1">Live workspace</p>
            <h1 className="text-[30px] leading-none tracking-[-.04em] font-medium truncate">
              {meetingTitle || 'New capture'}
            </h1>
            <p className="mt-2 text-sm text-[var(--pt-text-secondary)]">
              {isRecording ? (isPaused ? 'Capture paused. Your audio stays on this device.' : 'Listening locally. Your words stay on this device.') : 'Begin speaking when you are ready.'}
            </p>
          </div>
          <div className="flex items-center gap-3 shrink-0">
            <span className={`pt-label inline-flex items-center gap-2 px-2 py-1 border rounded-[2px] ${isRecording ? 'bg-[var(--pt-accent-wash)] border-[var(--pt-accent-soft)] text-[var(--pt-text)]' : 'bg-[var(--pt-surface)] border-[var(--pt-border)] text-[var(--pt-text-tertiary)]'}`}>
              <span className={`w-1.5 h-1.5 rounded-full ${isRecording ? 'bg-[var(--pt-accent)]' : 'bg-[var(--pt-border-strong)]'}`} />
              {isRecording ? 'Local capture' : 'Private'}
            </span>
            <ButtonGroup>
                {transcripts?.length > 0 && (
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={copyTranscript}
                    title="Copy transcript"
                    className="rounded-[3px] border-[var(--pt-border-strong)] bg-[var(--pt-surface)] hover:bg-[var(--pt-accent-wash)] focus-visible:ring-[var(--pt-accent)]"
                  >
                    <Copy />
                    <span className='hidden md:inline'>
                      Copy
                    </span>
                  </Button>
                )}
                {transcriptModelConfig.provider === "localWhisper" &&
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={() => showModal('languageSettings')}
                    title="Language"
                    className="rounded-[3px] border-[var(--pt-border-strong)] bg-[var(--pt-surface)] hover:bg-[var(--pt-accent-wash)] focus-visible:ring-[var(--pt-accent)]"
                  >
                    <GlobeIcon />
                    <span className='hidden md:inline'>
                      Language
                    </span>
                  </Button>
                }
            </ButtonGroup>
          </div>
        </div>
      </div>

      {/* Permission Warning - Not needed on Linux */}
      {!isRecording && !isChecking && !isLinux && (
        <div className="flex justify-center px-6 pt-6">
          <PermissionWarning
            hasMicrophone={hasMicrophone}
            hasSystemAudio={hasSystemAudio}
            onRecheck={checkPermissions}
            isRechecking={isChecking}
          />
        </div>
      )}

      {/* Transcript content */}
      <div className="pb-36 pt-6">
        <div className="flex justify-center">
          <div className="w-full max-w-[760px] mx-6">
            <VirtualizedTranscriptView
              segments={segments}
              isRecording={isRecording}
              isPaused={isPaused}
              isProcessing={isProcessingStop}
              isStopping={isStopping}
              enableStreaming={isRecording}
              showConfidence={true}
            />
          </div>
        </div>
      </div>
    </div>
  );
}
