"use client";
import { useState, useEffect, useRef, useCallback, useMemo } from 'react';
import { motion } from 'framer-motion';
import { MeetingSpeaker, MeetingSummary, SpeakerJobComplete, SummaryProcessResponse } from '@/types';
import { useSidebar } from '@/components/Sidebar/SidebarProvider';
import Analytics from '@/lib/analytics';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import { errorMessage } from '@/lib/errors';
import { TranscriptPanel, type SpeakerToolsInput } from '@/components/MeetingDetails/TranscriptPanel';
import { useMeetingSpeakers } from '@/hooks/useMeetingSpeakers';
import { usePeople } from '@/hooks/usePeople';
import { PeopleContext } from '@/components/Speakers/PeopleContext';
import { decideAutoGuessNames, isWaitingForSpeakers } from '@/lib/speakerNaming';
import { useSpeakerIdentification } from '@/hooks/useSpeakerIdentification';
import { SummaryPanel } from '@/components/MeetingDetails/SummaryPanel';
import { MeetingDetailsSplitView, type MeetingDetailsTab } from '@/components/MeetingDetails/MeetingDetailsSplitView';
import { ModelConfig } from '@/components/ModelSettingsModal';

// Custom hooks
import { useMeetingData } from '@/hooks/meeting-details/useMeetingData';
import { useSummaryGeneration } from '@/hooks/meeting-details/useSummaryGeneration';
import { useTemplates } from '@/hooks/meeting-details/useTemplates';
import { useCopyOperations } from '@/hooks/meeting-details/useCopyOperations';
import { useMeetingOperations } from '@/hooks/meeting-details/useMeetingOperations';
import { useConfig } from '@/contexts/ConfigContext';

export default function PageContent({
  meeting,
  summaryData,
  initialSummary,
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
  onSpeakerChange,
}: {
  meeting: any;
  summaryData: MeetingSummary | null;
  initialSummary: SummaryProcessResponse | null;
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
  onSpeakerChange?: (change: { transcriptId?: string; fromKey?: string; toKey: string }) => void;
}) {
  console.log('📄 PAGE CONTENT: Initializing with data:', {
    meetingId: meeting.id,
    summaryDataKeys: summaryData ? Object.keys(summaryData) : null,
    transcriptsCount: meeting.transcripts?.length
  });

  // State
  const [customPrompt, setCustomPrompt] = useState<string>('');
  const isRecording = false;
  const [activeTab, setActiveTab] = useState<MeetingDetailsTab>('transcript');

  // Ref to store the modal open function from SummaryGeneratorButtonGroup
  const openModelSettingsRef = useRef<(() => void) | null>(null);
  const autoSwitchedSummaryMeetingIdsRef = useRef(new Set<string>());
  const manuallySelectedTabMeetingIdsRef = useRef(new Set<string>());
  const autoGenerationStartedMeetingIdRef = useRef<string | null>(null);

  // Sidebar context
  const { serverAddress } = useSidebar();

  // Get model config from ConfigContext
  const { modelConfig, setModelConfig, isModelConfigLoading, betaFeatures, isAutoSummary } = useConfig();

  // Custom hooks
  const meetingData = useMeetingData({ meeting, summaryData, onMeetingUpdated });
  const templates = useTemplates();

  // Speakers
  // Known people for the name autocomplete; naming can add one.
  const { people, refresh: refreshPeople } = usePeople(betaFeatures.speakerIdentification);
  const {
    speakers,
    names: speakerNames,
    refetch: refetchSpeakers,
    name: nameSpeaker,
    confirm: confirmSpeaker,
    reject: rejectSpeaker,
    merge: mergeSpeakers,
    reassign: reassignSpeaker,
  } = useMeetingSpeakers(meeting.id, refreshPeople);
  // Enhance (retranscription) and Identify both replace the meeting's speakers, so reload them with
  // the rows. Returns the fresh speakers for callers that decide on them.
  const refetchTranscriptsAndSpeakers = useCallback(async (): Promise<MeetingSpeaker[]> => {
    const [, fresh] = await Promise.all([onRefetchTranscripts?.(), refetchSpeakers()]);
    return fresh;
  }, [onRefetchTranscripts, refetchSpeakers]);
  // The panel's refetch prop returns nothing.
  const onRefetchTranscriptsAndSpeakers = useCallback(async () => {
    await refetchTranscriptsAndSpeakers();
  }, [refetchTranscriptsAndSpeakers]);
  // True from an Identify completion until its automatic name guess is requested or skipped; from
  // the request on, the hook's `autoNamingPending` holds the wait until the job's first event.
  const [namingDecisionPending, setNamingDecisionPending] = useState(false);
  const guessNamesRef = useRef<(automatic: boolean, allowCloud?: boolean) => Promise<void>>(async () => {});
  const onSpeakerJobComplete = useCallback(async (result: SpeakerJobComplete) => {
    if (result.kind === 'naming') {
      await Promise.all([refetchSpeakers(), refreshPeople()]);
      return;
    }
    setNamingDecisionPending(true);
    try {
      const fresh = await refetchTranscriptsAndSpeakers();
      const request = decideAutoGuessNames({
        speakerIdentification: betaFeatures.speakerIdentification,
        isAutoSummary,
        speakers: fresh,
      });
      if (request) {
        await guessNamesRef.current(true, request.allowCloud);
      }
    } catch (error) {
      console.error('Automatic name guessing did not start:', error);
    } finally {
      setNamingDecisionPending(false);
    }
  }, [refetchTranscriptsAndSpeakers, refetchSpeakers, refreshPeople, betaFeatures.speakerIdentification, isAutoSummary]);
  const speakerIdentification = useSpeakerIdentification(meeting.id, onSpeakerJobComplete);
  guessNamesRef.current = speakerIdentification.guessNames;
  const {
    job: speakerJob,
    isActive: speakerJobActive,
    start: startSpeakerIdentification,
    cancel: cancelSpeakerIdentification,
    guessNames,
  } = speakerIdentification;
  // Give automatic speaker identification, and the name guess after it, up to 120 s before auto-summarising.
  const [speakerWaitExpired, setSpeakerWaitExpired] = useState(false);
  useEffect(() => {
    if (!shouldAutoGenerate) return;
    const timer = setTimeout(() => setSpeakerWaitExpired(true), 120_000);
    return () => clearTimeout(timer);
  }, [shouldAutoGenerate]);
  const waitingForSpeakers = isWaitingForSpeakers({
    expired: speakerWaitExpired,
    statusKnown: speakerIdentification.statusKnown,
    isActive: speakerIdentification.isActive,
    autoNamingPending: namingDecisionPending || speakerIdentification.autoNamingPending,
  });
  const onGuessNames = useCallback(async () => {
    try {
      await guessNames(false);
    } catch (error) {
      toast.error(errorMessage(error, 'Failed to guess names'));
    }
  }, [guessNames]);
  const onMergeSpeakers = useCallback(async (fromKey: string, intoKey: string) => {
    await mergeSpeakers(fromKey, intoKey);
    onSpeakerChange?.({ fromKey, toKey: intoKey });
  }, [mergeSpeakers, onSpeakerChange]);
  const onReassignSpeaker = useCallback(async (transcriptId: string, key: string | null) => {
    const newKey = await reassignSpeaker(transcriptId, key);
    onSpeakerChange?.({ transcriptId, toKey: newKey });
  }, [reassignSpeaker, onSpeakerChange]);
  const onStartIdentify = useCallback(
    (numSpeakers: number | null) => startSpeakerIdentification(meeting.folder_path, numSpeakers),
    [startSpeakerIdentification, meeting.folder_path],
  );
  // Stable references let the memoised transcript rows skip re-rendering while the list scrolls;
  // the job stays out because it changes on every progress event, and the people list (it reaches
  // the name form through PeopleContext) because a refresh must not re-render the rows.
  const speakerTools = useMemo<SpeakerToolsInput>(() => ({
    speakers,
    names: speakerNames,
    // Edits made while a job runs would be overwritten by its final write.
    editable: betaFeatures.speakerIdentification && !speakerJobActive,
    onRename: nameSpeaker,
    onMerge: onMergeSpeakers,
    onReassign: onReassignSpeaker,
    onConfirm: confirmSpeaker,
    onReject: rejectSpeaker,
    onCancelJob: cancelSpeakerIdentification,
    onStartIdentify,
    onGuessNames,
  }), [
    speakers,
    speakerNames,
    betaFeatures.speakerIdentification,
    speakerJobActive,
    nameSpeaker,
    onMergeSpeakers,
    onReassignSpeaker,
    confirmSpeaker,
    rejectSpeaker,
    cancelSpeakerIdentification,
    onStartIdentify,
    onGuessNames,
  ]);

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

      toast.success('Model settings saved successfully');
    } catch (error) {
      console.error('Failed to save model config:', error);
      toast.error('Failed to save model settings');
    }
  };

  const summaryGeneration = useSummaryGeneration({
    initialSummary,
    meeting,
    transcripts: meetingData.transcripts,
    modelConfig: modelConfig,
    isModelConfigLoading,
    selectedTemplate: templates.selectedTemplate,
    onMeetingUpdated,
    updateMeetingTitle: meetingData.updateMeetingTitle,
    setAiSummary: meetingData.setAiSummary,
    onOpenModelSettings: handleOpenModelSettings,
    speakerNames,
  });

  const copyOperations = useCopyOperations({
    meeting,
    transcripts: meetingData.transcripts,
    meetingTitle: meetingData.meetingTitle,
    aiSummary: meetingData.aiSummary,
    blockNoteSummaryRef: meetingData.blockNoteSummaryRef,
    speakerNames,
  });

  const meetingOperations = useMeetingOperations({
    meeting,
  });

  // Track page view
  useEffect(() => {
    Analytics.trackPageView('meeting_details');
  }, []);

  useEffect(() => {
    if (
      (meetingData.aiSummary || summaryGeneration.summaryStatus === 'completed')
      && !autoSwitchedSummaryMeetingIdsRef.current.has(meeting.id)
      && !manuallySelectedTabMeetingIdsRef.current.has(meeting.id)
    ) {
      autoSwitchedSummaryMeetingIdsRef.current.add(meeting.id);
      setActiveTab('summary');
    }
  }, [meeting.id, meetingData.aiSummary, summaryGeneration.summaryStatus]);

  // Auto-generate only after the model configuration has settled.
  useEffect(() => {
    if (
      !shouldAutoGenerate
      || summaryGeneration.summaryStatus !== 'idle'
      || isModelConfigLoading
      || meetingData.transcripts.length === 0
      || autoGenerationStartedMeetingIdRef.current === meeting.id
      || waitingForSpeakers
    ) {
      return;
    }

    autoGenerationStartedMeetingIdRef.current = meeting.id;
    console.log(`🤖 Auto-generating summary with ${modelConfig.provider}/${modelConfig.model}...`);
    onAutoGenerateComplete?.();
    void summaryGeneration.handleGenerateSummary('');
  }, [
    shouldAutoGenerate,
    meeting.id,
    meetingData.transcripts.length,
    isModelConfigLoading,
    modelConfig.provider,
    modelConfig.model,
    summaryGeneration.handleGenerateSummary,
    summaryGeneration.summaryStatus,
    onAutoGenerateComplete,
    waitingForSpeakers,
  ]);

  return (
    <motion.div
      initial={{ opacity: 0, y: 20 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.3, ease: 'easeOut' }}
      className="flex flex-col h-screen min-w-0 bg-gray-50"
    >
      <div className="flex flex-1 min-w-0 overflow-hidden">
        <MeetingDetailsSplitView
          activeTab={activeTab}
          onTabChange={(tab) => {
            manuallySelectedTabMeetingIdsRef.current.add(meeting.id);
            setActiveTab(tab);
          }}
          transcript={
            <PeopleContext.Provider value={people}>
              <TranscriptPanel
                transcripts={meetingData.transcripts}
                customPrompt={customPrompt}
                onPromptChange={setCustomPrompt}
                onCopyTranscript={copyOperations.handleCopyTranscript}
                onOpenMeetingFolder={meetingOperations.handleOpenMeetingFolder}
                isRecording={isRecording}
                disableAutoScroll={true}
                usePagination={true}
                segments={segments}
                hasMore={hasMore}
                isLoadingMore={isLoadingMore}
                totalCount={totalCount}
                loadedCount={loadedCount}
                onLoadMore={onLoadMore}
                meetingId={meeting.id}
                meetingFolderPath={meeting.folder_path}
                onRefetchTranscripts={onRefetchTranscriptsAndSpeakers}
                speakerTools={speakerTools}
                speakerJob={speakerJob}
              />
            </PeopleContext.Provider>
          }
          summary={
            <SummaryPanel
              meeting={meeting}
              meetingTitle={meetingData.meetingTitle}
              summaryRef={meetingData.blockNoteSummaryRef}
              isSaving={meetingData.isSaving}
              isSummaryDirty={meetingData.isSummaryDirty}
              onSaveAll={meetingData.saveAllChanges}
              onCopySummary={copyOperations.handleCopySummary}
              aiSummary={meetingData.aiSummary}
              summaryStatus={summaryGeneration.summaryStatus}
              transcripts={meetingData.transcripts}
              modelConfig={modelConfig}
              setModelConfig={setModelConfig}
              onSaveModelConfig={handleSaveModelConfig}
              onGenerateSummary={summaryGeneration.handleGenerateSummary}
              onStopGeneration={summaryGeneration.handleStopGeneration}
              customPrompt={customPrompt}
              onSaveSummary={meetingData.handleSaveSummary}
              onSummaryChange={meetingData.handleSummaryChange}
              onDirtyChange={meetingData.setIsSummaryDirty}
              summaryError={summaryGeneration.summaryError}
              onRegenerateSummary={summaryGeneration.handleRegenerateSummary}
              getSummaryStatusMessage={summaryGeneration.getSummaryStatusMessage}
              availableTemplates={templates.availableTemplates}
              selectedTemplate={templates.selectedTemplate}
              onTemplateSelect={templates.handleTemplateSelection}
              isModelConfigLoading={isModelConfigLoading}
              onOpenModelSettings={handleRegisterModalOpen}
            />
          }
        />
      </div>
    </motion.div>
  );
}
