"use client";
import { useState, useEffect, useRef, useMemo, useCallback } from 'react';
import { Summary, SummaryResponse } from '@/types';
import { getMarkedMoments } from '@/lib/markedMoments';
import { useSidebar } from '@/components/Sidebar/SidebarProvider';
import Analytics from '@/lib/analytics';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import { ModelConfig } from '@/components/ModelSettingsModal';

// Custom hooks
import { useMeetingData } from '@/hooks/meeting-details/useMeetingData';
import { useSummaryGeneration } from '@/hooks/meeting-details/useSummaryGeneration';
import { useTemplates } from '@/hooks/meeting-details/useTemplates';
import { useCopyOperations } from '@/hooks/meeting-details/useCopyOperations';
import { useTelegramShare } from '@/hooks/meeting-details/useTelegramShare';
import { useMeetingOperations } from '@/hooks/meeting-details/useMeetingOperations';
import { useConfig } from '@/contexts/ConfigContext';
import { useT } from '@/lib/i18n';
import { MeetingConversation } from '@/components/MeetingConversation/MeetingConversation';

/**
 * Variant 2a: the meeting screen is a single vertical conversation. This layer
 * keeps the data/generation composition hooks (unchanged) and hands their state
 * to <MeetingConversation>, which renders the transcript pin, the summary as the
 * first assistant message, and the meeting-scoped chat thread. The former
 * two-panel layout (draggable transcript/summary split) and the summary context
 * field have been removed.
 */
export default function PageContent({
  meeting,
  summaryData,
  summaryLoadStatus = 'loaded',
  summaryLoadError = null,
  onRetrySummary,
  onSummaryDataChange,
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
  speakerCount = 0,
  onRenameSpeaker,
  onSpeakersDetected,
}: {
  meeting: any;
  summaryData: Summary | null;
  summaryLoadStatus?: 'loading' | 'loaded' | 'absent' | 'error';
  summaryLoadError?: string | null;
  onRetrySummary?: () => Promise<void> | void;
  onSummaryDataChange?: (summary: Summary | null) => void;
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
  speakerCount?: number;
  onRenameSpeaker?: (speakerId: number, displayName: string) => Promise<void> | void;
  onSpeakersDetected?: () => Promise<void> | void;
}) {
  const t = useT();

  // Marked moments captured during recording (elapsed seconds), keyed by the
  // saved meeting id in localStorage.
  const markedMoments = useMemo(() => getMarkedMoments(meeting.id), [meeting.id]);

  // Ref to store the modal open function from SummaryGeneratorButtonGroup.
  const openModelSettingsRef = useRef<(() => void) | null>(null);

  // Sidebar context
  const { serverAddress } = useSidebar();

  // Get model config from ConfigContext
  const { modelConfig, setModelConfig } = useConfig();

  // Custom hooks
  const meetingData = useMeetingData({ meeting, summaryData, onMeetingUpdated });
  const templates = useTemplates(meeting.id);
  const setAiSummaryDurably = useCallback((summary: Summary | null) => {
    meetingData.setAiSummary(summary);
    onSummaryDataChange?.(summary);
  }, [meetingData.setAiSummary, onSummaryDataChange]);

  // Register the model-settings modal open function (from SummaryGeneratorButtonGroup).
  const handleRegisterModalOpen = (openFn: () => void) => {
    openModelSettingsRef.current = openFn;
  };

  // Trigger the model-settings modal (from the "⋯" menu and error handlers).
  const handleOpenModelSettings = () => {
    if (openModelSettingsRef.current) {
      openModelSettingsRef.current();
    } else {
      console.warn('⚠️ Model settings modal not yet registered');
    }
  };

  // Save model config to backend database and sync via event.
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

      const { emit } = await import('@tauri-apps/api/event');
      await emit('model-config-updated', config);

      toast.success(t('Model settings saved successfully'));
    } catch (error) {
      console.error('Failed to save model config:', error);
      toast.error(t('Failed to save model settings'));
    }
  };

  const telegramShare = useTelegramShare({
    meeting,
    meetingTitle: meetingData.meetingTitle,
    aiSummary: meetingData.aiSummary,
    blockNoteSummaryRef: meetingData.blockNoteSummaryRef,
  });

  const summaryGeneration = useSummaryGeneration({
    meeting,
    transcripts: meetingData.transcripts,
    modelConfig: modelConfig,
    isModelConfigLoading: false, // ConfigContext loads on mount
    selectedTemplate: templates.selectedTemplate,
    onMeetingUpdated,
    setAiSummary: setAiSummaryDurably,
    onOpenModelSettings: handleOpenModelSettings,
    // No-op unless the user turned auto-share on; opens Telegram, never sends by itself.
    onSummaryGenerated: telegramShare.autoShareIfEnabled,
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

  // Variant 2a: summarization is user-initiated (pick a template in the summary
  // message), so there is no auto-generation on entry — the `shouldAutoGenerate`
  // flag from the recording flow is intentionally ignored here.

  const summaryResponse: SummaryResponse | null = null;
  const isSummaryDirty = meetingData.isTitleDirty || (meetingData.blockNoteSummaryRef.current?.isDirty || false);

  // The full prop set the summary panel needs — assembled here (as before) and
  // rendered as the first assistant message inside <MeetingConversation>.
  const summaryPanelProps = {
    meeting,
    meetingTitle: meetingData.meetingTitle,
    onTitleChange: meetingData.handleTitleChange,
    isEditingTitle: meetingData.isEditingTitle,
    onStartEditTitle: () => meetingData.setIsEditingTitle(true),
    onFinishEditTitle: () => meetingData.setIsEditingTitle(false),
    isTitleDirty: meetingData.isTitleDirty,
    summaryRef: meetingData.blockNoteSummaryRef,
    isSaving: meetingData.isSaving,
    onSaveAll: meetingData.saveAllChanges,
    onCopySummary: copyOperations.handleCopySummary,
    onShareSummaryToTelegram: telegramShare.shareSummary,
    canShareToTelegram: telegramShare.canShareToTelegram,
    isSharingToTelegram: telegramShare.isSharing,
    onOpenFolder: meetingOperations.handleOpenMeetingFolder,
    onDiscussSummary: () => {}, // overridden by MeetingConversation → focuses the composer
    aiSummary: meetingData.aiSummary,
    summaryLoadStatus,
    summaryLoadError,
    onRetrySummary,
    speakerAttributionStale: Boolean(
      (meetingData.aiSummary as any)?.summary_freshness?.speaker_attribution_stale,
    ),
    summaryStatus: summaryGeneration.summaryStatus,
    transcripts: meetingData.transcripts,
    modelConfig,
    setModelConfig,
    onSaveModelConfig: handleSaveModelConfig,
    onGenerateSummary: summaryGeneration.handleGenerateSummary,
    onStopGeneration: summaryGeneration.handleStopGeneration,
    customPrompt: '',
    summaryResponse,
    onSaveSummary: meetingData.handleSaveSummary,
    onSummaryChange: meetingData.handleSummaryChange,
    onDirtyChange: meetingData.setIsSummaryDirty,
    summaryError: summaryGeneration.summaryError,
    onRegenerateSummary: summaryGeneration.handleRegenerateSummary,
    getSummaryStatusMessage: summaryGeneration.getSummaryStatusMessage,
    availableTemplates: templates.availableTemplates,
    selectedTemplate: templates.selectedTemplate,
    onTemplateSelect: templates.handleTemplateSelection,
    templateSuggestion: templates.templateSuggestion,
    onDismissTemplateSuggestion: templates.dismissTemplateSuggestion,
    isModelConfigLoading: false,
    onOpenModelSettings: handleRegisterModalOpen,
  };

  return (
    <>
      <MeetingConversation
        meetingId={meeting.id}
        meeting={meeting}
        meetingTitle={meetingData.meetingTitle}
        onTitleChange={meetingData.handleTitleChange}
        isEditingTitle={meetingData.isEditingTitle}
        onStartEditTitle={() => meetingData.setIsEditingTitle(true)}
        onFinishEditTitle={() => meetingData.setIsEditingTitle(false)}
        summaryPanelProps={summaryPanelProps}
        hasSummary={!!meetingData.aiSummary}
        isSaving={meetingData.isSaving}
        isSummaryDirty={isSummaryDirty}
        onCopySummary={copyOperations.handleCopySummary}
        onSaveSummary={meetingData.saveAllChanges}
        onOpenModelSettings={handleOpenModelSettings}
        transcripts={meetingData.transcripts}
        segments={segments}
        hasMore={hasMore}
        isLoadingMore={isLoadingMore}
        totalCount={totalCount}
        loadedCount={loadedCount}
        onLoadMore={onLoadMore}
        meetingFolderPath={meeting.folder_path}
        onRefetchTranscripts={onRefetchTranscripts}
        onOpenMeetingFolder={meetingOperations.handleOpenMeetingFolder}
        seekToSeconds={seekToSeconds}
        markedMoments={markedMoments}
        speakersById={speakersById}
        speakerCount={speakerCount}
        onRenameSpeaker={onRenameSpeaker}
        onSpeakersDetected={onSpeakersDetected}
      />
    </>
  );
}
