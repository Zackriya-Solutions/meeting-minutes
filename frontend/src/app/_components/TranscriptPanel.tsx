import { VirtualizedTranscriptView } from '@/components/VirtualizedTranscriptView';
import { PermissionWarning } from '@/components/PermissionWarning';
import { Button } from '@/components/ui/button';
import { Copy, GlobeIcon, HardDriveDownload } from 'lucide-react';
import { useTranscripts } from '@/contexts/TranscriptContext';
import { useConfig } from '@/contexts/ConfigContext';
import { useRecordingState } from '@/contexts/RecordingStateContext';
import { usePermissionCheck } from '@/hooks/usePermissionCheck';
import { ModalType } from '@/hooks/useModalState';
import { useIsLinux } from '@/hooks/usePlatform';
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip';
import { useMemo } from 'react';

/**
 * The live capture surface. Header carries the machine facts (which model is
 * decoding, that it is doing so locally) alongside the actions — see
 * /PRODUCT.md principle 3, "make the local machine legible".
 */
interface TranscriptPanelProps {
  isProcessingStop: boolean;
  isStopping: boolean;
  showModal: (name: ModalType, message?: string) => void;
}

export function TranscriptPanel({
  isProcessingStop,
  isStopping,
  showModal,
}: TranscriptPanelProps) {
  const { transcripts, partialText, transcriptContainerRef, copyTranscript } = useTranscripts();
  const { transcriptModelConfig } = useConfig();
  const { isRecording, isPaused } = useRecordingState();
  const { checkPermissions, isChecking, hasSystemAudio, hasMicrophone } =
    usePermissionCheck();
  const isLinux = useIsLinux();

  const segments = useMemo(
    () =>
      transcripts.map((t) => ({
        id: t.id,
        timestamp: t.audio_start_time ?? 0,
        endTime: t.audio_end_time,
        text: t.text,
        confidence: t.confidence,
      })),
    [transcripts]
  );

  const isLocal = transcriptModelConfig.provider === 'local';

  return (
    // Not a scroll container — VirtualizedTranscriptView owns scrolling, and a
    // second one here fights the virtualizer's measurements.
    <div ref={transcriptContainerRef} className="flex min-w-0 flex-1 flex-col overflow-hidden bg-canvas">
      <header className="flex h-12 shrink-0 items-center gap-3 border-b border-line px-4">
        {/* Which model is decoding, and where. */}
        <Tooltip>
          <TooltipTrigger asChild>
            <span className="flex min-w-0 items-center gap-1.5 text-ink-muted">
              {isLocal && (
                <HardDriveDownload className="h-3.5 w-3.5 shrink-0" aria-hidden />
              )}
              <span className="readout truncate text-2xs">
                {transcriptModelConfig.model || 'No model selected'}
              </span>
            </span>
          </TooltipTrigger>
          <TooltipContent side="bottom">
            {isLocal
              ? 'Transcribing on this machine — no audio leaves it'
              : `Transcribing via ${transcriptModelConfig.provider}`}
          </TooltipContent>
        </Tooltip>

        <div className="ml-auto flex items-center gap-1">
          {isLocal && (
            <Button
              variant="ghost"
              size="sm"
              onClick={() => showModal('languageSettings')}
            >
              <GlobeIcon aria-hidden />
              <span className="hidden md:inline">Language</span>
            </Button>
          )}
          {transcripts?.length > 0 && (
            <Button variant="ghost" size="sm" onClick={copyTranscript}>
              <Copy aria-hidden />
              <span className="hidden md:inline">Copy</span>
            </Button>
          )}
        </div>
      </header>

      {!isRecording && !isChecking && !isLinux && (
        <div className="shrink-0 px-4 pt-4">
          <div className="mx-auto max-w-measure">
            <PermissionWarning
              hasMicrophone={hasMicrophone}
              hasSystemAudio={hasSystemAudio}
              onRecheck={checkPermissions}
              isRechecking={isChecking}
            />
          </div>
        </div>
      )}

      {/* pb leaves room for the floating transport */}
      <div className="min-h-0 flex-1 pb-24">
        <div className="mx-auto h-full max-w-measure">
          <VirtualizedTranscriptView
            segments={segments}
            isRecording={isRecording}
            isPaused={isPaused}
            isProcessing={isProcessingStop}
            isStopping={isStopping}
            enableStreaming={isRecording}
            partialText={partialText}
            showConfidence={true}
          />
        </div>
      </div>
    </div>
  );
}
