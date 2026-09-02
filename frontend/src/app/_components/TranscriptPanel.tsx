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
import { CaptureMode, CaptureModeSwitcher } from '@/components/CaptureModeSwitcher';
import { Check, Keyboard, LockKeyhole, MonitorUp, Sparkles } from 'lucide-react';

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
  captureMode: CaptureMode;
  onCaptureModeChange: (mode: CaptureMode) => void;
}

export function TranscriptPanel({
  isProcessingStop,
  isStopping,
  showModal,
  captureMode,
  onCaptureModeChange,
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
      <div className="sticky top-0 z-10 border-b border-[var(--pt-border)] bg-[rgba(247,246,242,.96)] px-5 py-5 md:px-8">
        <div className="mx-auto max-w-[1180px]">
          <div className="flex flex-col gap-5 lg:flex-row lg:items-start lg:justify-between">
            <div className="min-w-0">
              <p className="pt-label mb-1 text-[var(--pt-text-tertiary)]">Capture workspace</p>
              <h1 className="truncate text-[30px] font-medium leading-none tracking-[-.04em]">
                {captureMode === 'record-meeting' ? (meetingTitle || 'New meeting') : 'Type anywhere'}
              </h1>
              <p className="mt-2 max-w-xl text-sm text-[var(--pt-text-secondary)]">
                {captureMode === 'type-anywhere'
                  ? 'Turn speech into text in the application you are already using.'
                  : isRecording
                    ? (isPaused ? 'Capture paused. Your audio stays on this device.' : 'Listening locally. Your audio stays on this device.')
                    : 'Capture a conversation and keep the transcript on this device.'}
              </p>
            </div>
            <div className="flex shrink-0 items-center gap-3">
            <span className={`pt-label inline-flex items-center gap-2 px-2 py-1 border rounded-[2px] ${isRecording ? 'bg-[var(--pt-accent-wash)] border-[var(--pt-accent-soft)] text-[var(--pt-text)]' : 'bg-[var(--pt-surface)] border-[var(--pt-border)] text-[var(--pt-text-tertiary)]'}`}>
              <span className={`w-1.5 h-1.5 rounded-full ${isRecording ? 'bg-[var(--pt-accent)]' : 'bg-[var(--pt-border-strong)]'}`} />
              {isRecording ? 'Recording locally' : 'Local and private'}
            </span>
            {captureMode === 'record-meeting' && <ButtonGroup>
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
            </ButtonGroup>}
            </div>
          </div>
          <div className="mt-5 max-w-[760px]">
            <CaptureModeSwitcher
              value={captureMode}
              onChange={onCaptureModeChange}
              meetingModeLocked={isRecording || isStopping || isProcessingStop}
            />
            {(isRecording || isStopping || isProcessingStop) && (
              <p className="mt-2 text-xs text-[var(--pt-text-tertiary)]" role="status">
                Finish this meeting before changing capture mode.
              </p>
            )}
          </div>
        </div>
      </div>

      {captureMode === 'record-meeting' && !isRecording && !isChecking && !isLinux && (
        <div className="flex justify-center px-6 pt-6">
          <PermissionWarning
            hasMicrophone={hasMicrophone}
            hasSystemAudio={hasSystemAudio}
            onRecheck={checkPermissions}
            isRechecking={isChecking}
          />
        </div>
      )}

      {captureMode === 'type-anywhere' ? (
        <section
          id="capture-panel-type-anywhere"
          role="tabpanel"
          aria-labelledby="capture-mode-type-anywhere"
          className="mx-auto w-full max-w-[1180px] px-5 py-8 md:px-8"
        >
          <div className="grid gap-4 lg:grid-cols-[minmax(0,1.5fr)_minmax(280px,.7fr)]">
            <div className="pt-card p-5 md:p-6">
              <div className="flex items-start justify-between gap-4">
                <div>
                  <p className="pt-label text-[var(--pt-accent-active)]">Workflow preview</p>
                  <h2 className="mt-2 text-[21px] font-medium tracking-[-.025em]">Speak without leaving your work</h2>
                </div>
                <Keyboard aria-hidden="true" className="shrink-0 text-[var(--pt-text-tertiary)]" size={22} />
              </div>
              <ol className="mt-6 grid gap-3 sm:grid-cols-3">
                {[
                  ['01', 'Hold the shortcut', 'Start from any text field.'],
                  ['02', 'Speak naturally', 'Transcription runs on this device.'],
                  ['03', 'Release to insert', 'Text appears at your cursor.'],
                ].map(([step, title, body]) => (
                  <li key={step} className="border-t-2 border-[var(--pt-border)] pt-3">
                    <span className="pt-label text-[var(--pt-text-tertiary)]">{step}</span>
                    <h3 className="mt-3 text-sm font-medium text-[var(--pt-text)]">{title}</h3>
                    <p className="mt-1 text-sm leading-5 text-[var(--pt-text-secondary)]">{body}</p>
                  </li>
                ))}
              </ol>
              <div className="mt-6 flex flex-col gap-3 border border-[var(--pt-border)] bg-[var(--pt-surface-alt)] p-4 sm:flex-row sm:items-center sm:justify-between">
                <div>
                  <p className="text-sm font-medium text-[var(--pt-text)]">Global shortcut</p>
                  <p className="mt-1 text-xs text-[var(--pt-text-tertiary)]">The native shortcut service is still in development.</p>
                </div>
                <div className="flex items-center gap-2" aria-label="Planned keyboard shortcut">
                  <kbd className="min-w-9 border border-[var(--pt-border-strong)] bg-[var(--pt-surface)] px-2 py-1.5 text-center text-xs font-medium shadow-[0_1px_0_var(--pt-border-strong)]">Ctrl</kbd>
                  <span aria-hidden="true" className="text-[var(--pt-text-tertiary)]">+</span>
                  <kbd className="min-w-9 border border-[var(--pt-border-strong)] bg-[var(--pt-surface)] px-2 py-1.5 text-center text-xs font-medium shadow-[0_1px_0_var(--pt-border-strong)]">Space</kbd>
                </div>
              </div>
            </div>

            <aside className="pt-card p-5" aria-labelledby="type-anywhere-readiness">
              <div className="flex items-center justify-between gap-3">
                <h2 id="type-anywhere-readiness" className="text-base font-medium">Readiness</h2>
                <span className="pt-badge pt-badge--warning">Coming soon</span>
              </div>
              <ul className="mt-5 space-y-4 text-sm">
                <li className="flex gap-3">
                  <Check aria-hidden="true" className="mt-0.5 shrink-0 text-[var(--pt-success)]" size={16} />
                  <span><span className="block font-medium">Local transcription</span><span className="mt-0.5 block text-xs text-[var(--pt-text-tertiary)]">Speech processing is available on device.</span></span>
                </li>
                <li className="flex gap-3">
                  <LockKeyhole aria-hidden="true" className="mt-0.5 shrink-0 text-[var(--pt-success)]" size={16} />
                  <span><span className="block font-medium">Private by default</span><span className="mt-0.5 block text-xs text-[var(--pt-text-tertiary)]">Audio does not need to leave your computer.</span></span>
                </li>
                <li className="flex gap-3">
                  <MonitorUp aria-hidden="true" className="mt-0.5 shrink-0 text-[var(--pt-warning)]" size={16} />
                  <span><span className="block font-medium">Text insertion</span><span className="mt-0.5 block text-xs text-[var(--pt-text-tertiary)]">Native app targeting is not connected yet.</span></span>
                </li>
              </ul>
              <button type="button" disabled className="pt-button mt-6 w-full" aria-describedby="type-anywhere-status">
                <Sparkles aria-hidden="true" size={16} />
                Enable type anywhere
              </button>
              <p id="type-anywhere-status" className="mt-2 text-center text-xs text-[var(--pt-text-tertiary)]">
                Preview only. No shortcut will be registered.
              </p>
            </aside>
          </div>
        </section>
      ) : <div
        id="capture-panel-record-meeting"
        role="tabpanel"
        aria-labelledby="capture-mode-record-meeting"
        className="pb-36 pt-6"
      >
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
      </div>}
    </div>
  );
}
