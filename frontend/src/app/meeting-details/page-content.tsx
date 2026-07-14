"use client";
import { useState, useEffect, useRef, useMemo } from 'react';
import { motion } from 'framer-motion';
import { Summary, SummaryResponse } from '@/types';
import { getMarkedMoments } from '@/lib/markedMoments';
import { useSidebar } from '@/components/Sidebar/SidebarProvider';
import Analytics from '@/lib/analytics';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import { TranscriptPanel } from '@/components/MeetingDetails/TranscriptPanel';
import { SummaryPanel } from '@/components/MeetingDetails/SummaryPanel';
import { ModelConfig } from '@/components/ModelSettingsModal';

// Custom hooks
import { useMeetingData } from '@/hooks/meeting-details/useMeetingData';
import { useSummaryGeneration } from '@/hooks/meeting-details/useSummaryGeneration';
import { useTemplates } from '@/hooks/meeting-details/useTemplates';
import { useCopyOperations } from '@/hooks/meeting-details/useCopyOperations';
import { useMeetingOperations } from '@/hooks/meeting-details/useMeetingOperations';
import { useConfig } from '@/contexts/ConfigContext';
import { useT } from '@/lib/i18n';

// Transcript/summary splitter: user-dragged width of the transcript pane (px),
// persisted across meetings. Bounded so neither pane can be crushed.
const TRANSCRIPT_WIDTH_KEY = 'memento:details:transcript-width';
const MIN_TRANSCRIPT_WIDTH = 260;
const MAX_TRANSCRIPT_FRACTION = 0.65;

function readStoredTranscriptWidth(): number | null {
  if (typeof window === 'undefined') return null;
  try {
    const parsed = Number(window.localStorage.getItem(TRANSCRIPT_WIDTH_KEY));
    return Number.isFinite(parsed) && parsed >= MIN_TRANSCRIPT_WIDTH ? parsed : null;
  } catch {
    return null;
  }
}

export default function PageContent({
  meeting,
  summaryData,
  shouldAutoGenerate = false,
  onAutoGenerateComplete,
  onMeetingUpdated,
  onRefetchTranscripts,
  // Pagination props for efficient transcript loading
  segments,
  hasMore,
  isLoadingMore,
  totalCount,
  loadedCount,
  onLoadMore,
  seekToSeconds = null,
  speakersById = null,
  onRenameSpeaker,
  onSpeakersDetected,
}: {
  meeting: any;
  summaryData: Summary | null;
  shouldAutoGenerate?: boolean;
  onAutoGenerateComplete?: () => void;
  onMeetingUpdated?: () => Promise<void>;
  onRefetchTranscripts?: () => Promise<void>;
  // Pagination props
  segments?: any[];
  hasMore?: boolean;
  isLoadingMore?: boolean;
  totalCount?: number;
  loadedCount?: number;
  onLoadMore?: () => void;
  /** Jump-to-timestamp (seconds) from search/RAG deep links (?t=). */
  seekToSeconds?: number | null;
  // Speaker diarization props
  speakersById?: Map<number, string> | null;
  onRenameSpeaker?: (speakerId: number, displayName: string) => Promise<void> | void;
  onSpeakersDetected?: () => Promise<void> | void;
}) {
  console.log('📄 PAGE CONTENT: Initializing with data:', {
    meetingId: meeting.id,
    summaryDataKeys: summaryData ? Object.keys(summaryData) : null,
    transcriptsCount: meeting.transcripts?.length
  });

  const t = useT();

  // State
  const [customPrompt, setCustomPrompt] = useState<string>('');
  const [isRecording] = useState(false);
  const [summaryResponse] = useState<SummaryResponse | null>(null);

  // Marked moments captured during recording (elapsed seconds), keyed by the
  // saved meeting id in localStorage.
  const markedMoments = useMemo(() => getMarkedMoments(meeting.id), [meeting.id]);

  // Transcript jump target (seconds). Driven by the ?t= deep link and by
  // clicking a marked-moment chip. A tiny jitter is added per click so tapping
  // the same moment twice still re-triggers the scroll (the transcript view
  // dedupes identical values).
  const [seekTarget, setSeekTarget] = useState<number | null>(seekToSeconds);
  useEffect(() => {
    if (seekToSeconds != null) setSeekTarget(seekToSeconds);
  }, [seekToSeconds]);
  const handleSeekToMoment = (seconds: number) => {
    setSeekTarget(seconds + Math.random() * 0.02);
  };

  // Ref to store the modal open function from SummaryGeneratorButtonGroup
  const openModelSettingsRef = useRef<(() => void) | null>(null);

  // Draggable vertical splitter between the transcript and summary panes.
  // Pointer capture keeps the drag alive over the BlockNote editor; the width
  // is clamped live against the container so window resizes can't wedge the
  // layout, and `min(px, 65%)` re-clamps rendered width without JS on resize.
  const splitContainerRef = useRef<HTMLDivElement>(null);
  const transcriptPaneRef = useRef<HTMLDivElement>(null);
  const [transcriptWidth, setTranscriptWidth] = useState<number | null>(readStoredTranscriptWidth);

  const handleSplitterPointerDown = (e: React.PointerEvent<HTMLDivElement>) => {
    const pane = transcriptPaneRef.current;
    if (!pane) return;
    e.preventDefault();
    const splitter = e.currentTarget;
    splitter.setPointerCapture(e.pointerId);
    const startX = e.clientX;
    const startWidth = pane.getBoundingClientRect().width;
    document.body.style.cursor = 'col-resize';
    document.body.style.userSelect = 'none';

    let width = startWidth;
    const onMove = (ev: PointerEvent) => {
      const container = splitContainerRef.current;
      const max = container
        ? Math.max(MIN_TRANSCRIPT_WIDTH, container.getBoundingClientRect().width * MAX_TRANSCRIPT_FRACTION)
        : startWidth;
      width = Math.min(Math.max(startWidth + (ev.clientX - startX), MIN_TRANSCRIPT_WIDTH), max);
      setTranscriptWidth(width);
    };
    const onEnd = () => {
      splitter.removeEventListener('pointermove', onMove);
      splitter.removeEventListener('pointerup', onEnd);
      splitter.removeEventListener('pointercancel', onEnd);
      document.body.style.cursor = '';
      document.body.style.userSelect = '';
      try {
        window.localStorage.setItem(TRANSCRIPT_WIDTH_KEY, String(Math.round(width)));
      } catch { /* ignore */ }
    };
    splitter.addEventListener('pointermove', onMove);
    splitter.addEventListener('pointerup', onEnd);
    splitter.addEventListener('pointercancel', onEnd);
  };

  // Double-click restores the default 33% split.
  const resetSplitter = () => {
    setTranscriptWidth(null);
    try {
      window.localStorage.removeItem(TRANSCRIPT_WIDTH_KEY);
    } catch { /* ignore */ }
  };

  // Sidebar context
  const { serverAddress } = useSidebar();

  // Get model config from ConfigContext
  const { modelConfig, setModelConfig } = useConfig();

  // Custom hooks
  const meetingData = useMeetingData({ meeting, summaryData, onMeetingUpdated });
  const templates = useTemplates();

  // Callback to register the modal open function
  const handleRegisterModalOpen = (openFn: () => void) => {
    console.log('📝 Registering modal open function in PageContent');
    openModelSettingsRef.current = openFn;
  };

  // Callback to trigger modal open (called from error handler)
  const handleOpenModelSettings = () => {
    console.log('🔔 Opening model settings from PageContent');
    if (openModelSettingsRef.current) {
      openModelSettingsRef.current();
    } else {
      console.warn('⚠️ Modal open function not yet registered');
    }
  };

  // Save model config to backend database and sync via event
  const handleSaveModelConfig = async (config?: ModelConfig) => {
    if (!config) return;
    try {
      await invoke('api_save_model_config', {
        provider: config.provider,
        model: config.model,
        whisperModel: config.whisperModel,
        apiKey: config.apiKey ?? null,
        ollamaEndpoint: config.ollamaEndpoint ?? null,
      });

      // Emit event so ConfigContext and other listeners stay in sync
      const { emit } = await import('@tauri-apps/api/event');
      await emit('model-config-updated', config);

      toast.success(t('Model settings saved successfully'));
    } catch (error) {
      console.error('Failed to save model config:', error);
      toast.error(t('Failed to save model settings'));
    }
  };

  const summaryGeneration = useSummaryGeneration({
    meeting,
    transcripts: meetingData.transcripts,
    modelConfig: modelConfig,
    isModelConfigLoading: false, // ConfigContext loads on mount
    selectedTemplate: templates.selectedTemplate,
    onMeetingUpdated,
    updateMeetingTitle: meetingData.updateMeetingTitle,
    setAiSummary: meetingData.setAiSummary,
    onOpenModelSettings: handleOpenModelSettings,
  });

  const copyOperations = useCopyOperations({
    meeting,
    transcripts: meetingData.transcripts,
    meetingTitle: meetingData.meetingTitle,
    aiSummary: meetingData.aiSummary,
    blockNoteSummaryRef: meetingData.blockNoteSummaryRef,
  });

  const meetingOperations = useMeetingOperations({
    meeting,
  });

  // Track page view
  useEffect(() => {
    Analytics.trackPageView('meeting_details');
  }, []);

  // Auto-generate summary when flag is set
  useEffect(() => {
    let cancelled = false;

    const autoGenerate = async () => {
      if (shouldAutoGenerate && meetingData.transcripts.length > 0 && !cancelled) {
        console.log(`🤖 Auto-generating summary with ${modelConfig.provider}/${modelConfig.model}...`);
        await summaryGeneration.handleGenerateSummary('');

        // Notify parent that auto-generation is complete (only if not cancelled)
        if (onAutoGenerateComplete && !cancelled) {
          onAutoGenerateComplete();
        }
      }
    };

    autoGenerate();

    // Cleanup: cancel if component unmounts or meeting changes
    return () => {
      cancelled = true;
    };
  }, [shouldAutoGenerate, meeting.id]); // Re-run if meeting changes

  return (
    <motion.div
      initial={{ opacity: 0, y: 20 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.3, ease: 'easeOut' }}
      className="flex h-screen flex-col bg-[var(--bg-canvas)]"
    >
      <div
        ref={splitContainerRef}
        className="m-4 flex flex-1 overflow-hidden rounded-[24px] border border-[var(--border-subtle)] bg-[var(--bg-sheet)]"
      >
        <div
          ref={transcriptPaneRef}
          className="flex min-w-0 shrink-0"
          style={{
            width:
              transcriptWidth != null
                ? `min(${transcriptWidth}px, ${MAX_TRANSCRIPT_FRACTION * 100}%)`
                : '33%',
            minWidth: MIN_TRANSCRIPT_WIDTH,
          }}
        >
        <TranscriptPanel
          transcripts={meetingData.transcripts}
          customPrompt={customPrompt}
          onPromptChange={setCustomPrompt}
          onCopyTranscript={copyOperations.handleCopyTranscript}
          onOpenMeetingFolder={meetingOperations.handleOpenMeetingFolder}
          isRecording={isRecording}
          disableAutoScroll={true}
          // Pagination props for efficient loading
          usePagination={true}
          segments={segments}
          hasMore={hasMore}
          isLoadingMore={isLoadingMore}
          totalCount={totalCount}
          loadedCount={loadedCount}
          onLoadMore={onLoadMore}
          // Retranscription props
          meetingId={meeting.id}
          meetingFolderPath={meeting.folder_path}
          onRefetchTranscripts={onRefetchTranscripts}
          // Jump-to-timestamp: ?t= deep link (search/RAG) and marked moments
          scrollToTimestamp={seekTarget}
          markedMoments={markedMoments}
          onSeekToMoment={handleSeekToMoment}
          // Speaker diarization props
          speakersById={speakersById}
          onRenameSpeaker={onRenameSpeaker}
          onSpeakersDetected={onSpeakersDetected}
        />
        </div>
        <div
          role="separator"
          aria-orientation="vertical"
          aria-label={t('Resize transcript panel')}
          title={t('Resize transcript panel')}
          onPointerDown={handleSplitterPointerDown}
          onDoubleClick={resetSplitter}
          className="group relative z-10 w-[7px] shrink-0 cursor-col-resize touch-none"
        >
          <div className="absolute inset-y-0 left-1/2 w-px -translate-x-1/2 bg-[var(--border-subtle)] transition-all group-hover:w-[3px] group-hover:bg-[var(--gold-border)]" />
        </div>
        <SummaryPanel
          meeting={meeting}
          meetingTitle={meetingData.meetingTitle}
          onTitleChange={meetingData.handleTitleChange}
          isEditingTitle={meetingData.isEditingTitle}
          onStartEditTitle={() => meetingData.setIsEditingTitle(true)}
          onFinishEditTitle={() => meetingData.setIsEditingTitle(false)}
          isTitleDirty={meetingData.isTitleDirty}
          summaryRef={meetingData.blockNoteSummaryRef}
          isSaving={meetingData.isSaving}
          onSaveAll={meetingData.saveAllChanges}
          onCopySummary={copyOperations.handleCopySummary}
          onOpenFolder={meetingOperations.handleOpenMeetingFolder}
          aiSummary={meetingData.aiSummary}
          summaryStatus={summaryGeneration.summaryStatus}
          transcripts={meetingData.transcripts}
          modelConfig={modelConfig}
          setModelConfig={setModelConfig}
          onSaveModelConfig={handleSaveModelConfig}
          onGenerateSummary={summaryGeneration.handleGenerateSummary}
          onStopGeneration={summaryGeneration.handleStopGeneration}
          customPrompt={customPrompt}
          summaryResponse={summaryResponse}
          onSaveSummary={meetingData.handleSaveSummary}
          onSummaryChange={meetingData.handleSummaryChange}
          onDirtyChange={meetingData.setIsSummaryDirty}
          summaryError={summaryGeneration.summaryError}
          onRegenerateSummary={summaryGeneration.handleRegenerateSummary}
          getSummaryStatusMessage={summaryGeneration.getSummaryStatusMessage}
          availableTemplates={templates.availableTemplates}
          selectedTemplate={templates.selectedTemplate}
          onTemplateSelect={templates.handleTemplateSelection}
          isModelConfigLoading={false}
          onOpenModelSettings={handleRegisterModalOpen}
        />
      </div>
    </motion.div>
  );
}
