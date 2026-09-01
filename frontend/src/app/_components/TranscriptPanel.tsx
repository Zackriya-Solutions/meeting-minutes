import { VirtualizedTranscriptView } from '@/components/VirtualizedTranscriptView';
import { PermissionWarning } from '@/components/PermissionWarning';
import { Button } from '@/components/ui/button';
import { ButtonGroup } from '@/components/ui/button-group';
import { Copy, GlobeIcon, Languages } from 'lucide-react';
import { Switch } from '@/components/ui/switch';
import { TRANSLATION_LANGUAGE_OPTIONS } from '@/lib/summary-languages';
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
  const { transcripts, annotations, addAnnotation, transcriptContainerRef, copyTranscript } = useTranscripts();
  const {
    transcriptModelConfig,
    translationSettings,
    translationEnabled,
    setTranslationEnabled,
    setTranslationTarget,
  } = useConfig();
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
      source: t.source,
      confidence: t.confidence,
      translation: t.translation,
      translationTargetLanguage: t.translation_target_language,
      translationStatus: t.translation_status ?? (t.translation ? 'ready' : undefined),
      translationError: t.translation_error,
    })),
    [transcripts]
  );

  return (
    <div ref={transcriptContainerRef} className="w-full border-r border-gray-200 bg-white flex flex-col overflow-y-auto">
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
                    onClick={copyTranscript}
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
                    onClick={() => showModal('languageSettings')}
                    title="Language"
                  >
                    <GlobeIcon />
                    <span className='hidden md:inline'>
                      Language
                    </span>
                  </Button>
                }
              </ButtonGroup>
              <div className="flex items-center gap-2 text-xs text-gray-600">
                <Languages className="h-4 w-4" aria-hidden="true" />
                <label htmlFor="live-translation-toggle">Translate</label>
                <Switch
                  id="live-translation-toggle"
                  checked={translationEnabled}
                  onCheckedChange={checked => {
                    setTranslationEnabled(checked).catch(error => {
                      console.error('Failed to toggle live translation:', error);
                    });
                  }}
                  disabled={!isRecording || isStopping || isProcessingStop}
                  aria-label="Toggle live translation"
                />
                <select
                  value={translationSettings.targetLanguage}
                  onChange={event => setTranslationTarget(event.target.value).catch(error => console.error('Failed to change translation target:', error))}
                  disabled={!translationEnabled || !isRecording || isStopping || isProcessingStop}
                  className="h-8 rounded-md border border-gray-200 bg-white px-2 text-xs disabled:cursor-not-allowed disabled:opacity-50"
                  aria-label="Translation target language"
                >
                  {TRANSLATION_LANGUAGE_OPTIONS.map(language => (
                    <option key={language.code} value={language.code}>{language.label}</option>
                  ))}
                </select>
              </div>
            </div>
          </div>
        </div>
      </div>

      {/* Permission Warning - Not needed on Linux */}
      {!isRecording && !isChecking && !isLinux && (
        <div className="flex justify-center px-4 pt-4">
          <PermissionWarning
            hasMicrophone={hasMicrophone}
            hasSystemAudio={hasSystemAudio}
            onRecheck={checkPermissions}
            isRechecking={isChecking}
          />
        </div>
      )}

      {/* Transcript content */}
      <div className="pb-20">
        <div className="flex justify-center">
          <div className="w-2/3 max-w-[750px]">
            <VirtualizedTranscriptView
              segments={segments}
              isRecording={isRecording}
              isPaused={isPaused}
              isProcessing={isProcessingStop}
              isStopping={isStopping}
              enableStreaming={isRecording}
              showConfidence={true}
              annotations={annotations}
              onAddAnnotation={addAnnotation}
            />
          </div>
        </div>
      </div>
    </div>
  );
}
