import { Transcript } from '@/types';
import { TranscriptView } from '@/components/TranscriptView';
import { PermissionWarning } from '@/components/PermissionWarning';
import { TranscriptModelProps } from '@/components/TranscriptSettings';
import { Button } from '@/components/ui/button';
import { ButtonGroup } from '@/components/ui/button-group';
import { Copy, GlobeIcon } from 'lucide-react';

// The props would get updated after the full refactor(hooks, context)
// TranscriptPanel Component Extracted from page.tsx
// Usage example:
{/* <TranscriptPanel
  transcripts={transcripts}
  isRecording={recordingState.isRecording}
  isPaused={recordingState.isPaused}
  isProcessingStop={isProcessingStop}
  isStopping={isStopping}
  hasMicrophone={hasMicrophone}
  hasSystemAudio={hasSystemAudio}
  isCheckingPermissions={isCheckingPermissions}
  onCheckPermissions={checkPermissions}
  transcriptModelConfig={transcriptModelConfig}
  onCopyTranscript={handleCopyTranscript}
  onOpenLanguageSettings={() => setShowLanguageSettings(true)}
  containerRef={transcriptContainerRef}
 /> */}

interface TranscriptPanelProps {
  // Transcript data
  transcripts: Transcript[];

  // Recording state
  isRecording: boolean;
  isPaused: boolean;
  isProcessingStop: boolean;
  isStopping: boolean;

  // Permission state
  hasMicrophone: boolean;
  hasSystemAudio: boolean;
  isCheckingPermissions: boolean;
  onCheckPermissions: () => void;

  // Transcript model config
  transcriptModelConfig: TranscriptModelProps;

  // Callbacks
  onCopyTranscript: () => void;
  onOpenLanguageSettings: () => void;

  // Ref for scroll container
  containerRef: React.RefObject<HTMLDivElement>;
}

export function TranscriptPanel({
  transcripts,
  isRecording,
  isPaused,
  isProcessingStop,
  isStopping,
  hasMicrophone,
  hasSystemAudio,
  isCheckingPermissions,
  onCheckPermissions,
  transcriptModelConfig,
  onCopyTranscript,
  onOpenLanguageSettings,
  containerRef,
}: TranscriptPanelProps) {
  return (
    <div ref={containerRef} className="w-full border-r border-gray-200 bg-white flex flex-col overflow-y-auto">
      {/* Title area - Sticky header */}
      <div className="sticky top-0 z-10 bg-white p-4 border-gray-200">
        <div className="flex flex-col space-y-3">
          <div className="flex  flex-col space-y-2">
            <div className="flex justify-center  items-center space-x-2">
              <ButtonGroup>
                {transcripts?.length > 0 && (
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={onCopyTranscript}
                    title="Copy Transcript"
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
                    onClick={onOpenLanguageSettings}
                    title="Language"
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
      </div>

      {/* Permission Warning */}
      {!isRecording && !isCheckingPermissions && (
        <div className="flex justify-center px-4 pt-4">
          <PermissionWarning
            hasMicrophone={hasMicrophone}
            hasSystemAudio={hasSystemAudio}
            onRecheck={onCheckPermissions}
            isRechecking={isCheckingPermissions}
          />
        </div>
      )}

      {/* Transcript content */}
      <div className="pb-20">
        <div className="flex justify-center">
          <div className="w-2/3 max-w-[750px]">
            <TranscriptView
              transcripts={transcripts}
              isRecording={isRecording}
              isPaused={isPaused}
              isProcessing={isProcessingStop}
              isStopping={isStopping}
              enableStreaming={isRecording}
            />
          </div>
        </div>
      </div>
    </div>
  );
}
