import { useState, useCallback, useRef } from 'react';
import {
  MeetingSummary,
  ProcessTranscriptResponse,
  SummaryProcessResponse,
  Transcript,
} from '@/types';
import { ModelConfig } from '@/components/ModelSettingsModal';
import { useSidebar } from '@/components/Sidebar/SidebarProvider';
import { invoke as invokeTauri } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import Analytics from '@/lib/analytics';

import {
  detectAndCacheSummaryLanguage,
  readMeetingSummaryLanguage,
  readCachedDetectedSummaryLanguage,
} from '@/lib/summary-language-preferences';
import { parseSummaryContent, readSummaryMetadata } from '@/lib/summary-content';

async function resolveSummaryLanguage(
  meetingId: string,
  transcriptTexts: string[]
): Promise<string | null> {
  try {
    const perMeeting = await readMeetingSummaryLanguage(meetingId);
    if (perMeeting.language) return perMeeting.language;
  } catch (err) {
    console.warn('Failed to load meeting summary language:', err);
    toast.warning('Could not load saved summary language', {
      description: 'Using Auto for this generation.',
    });
  }

  try {
    const cachedDetected = await readCachedDetectedSummaryLanguage(meetingId);
    if (cachedDetected) return cachedDetected;
  } catch (err) {
    console.warn('Failed to load cached detected summary language:', err);
  }

  try {
    const detection = await detectAndCacheSummaryLanguage(meetingId, transcriptTexts);
    if (detection.reason === 'tie') {
      toast.warning('Bilingual transcript detected', {
        description: 'Pick a summary language manually if Auto chooses the wrong fallback.',
      });
    }
    return detection.language;
  } catch (err) {
    console.warn('Failed to detect transcript summary language:', err);
    return null;
  }
}

type SummaryStatus = 'idle' | 'processing' | 'summarizing' | 'regenerating' | 'completed' | 'error';

interface UseSummaryGenerationProps {
  meeting: {
    id: string;
    created_at: string;
  };
  transcripts: Transcript[];
  modelConfig: ModelConfig;
  isModelConfigLoading: boolean;
  selectedTemplate: string;
  onMeetingUpdated?: () => Promise<void>;
  updateMeetingTitle: (title: string) => void;
  setAiSummary: (summary: MeetingSummary | null) => void;
  onOpenModelSettings?: () => void;
}

export function useSummaryGeneration({
  meeting,
  transcripts,
  modelConfig,
  isModelConfigLoading,
  selectedTemplate,
  onMeetingUpdated,
  updateMeetingTitle,
  setAiSummary,
  onOpenModelSettings,
}: UseSummaryGenerationProps) {
  const [summaryStatus, setSummaryStatus] = useState<SummaryStatus>('idle');
  const [summaryError, setSummaryError] = useState<string | null>(null);
  const generationIdRef = useRef(0);
  const activeProcessIdRef = useRef<string | null>(null);
  const trackedAttemptRef = useRef<{
    generationId: number;
    startedAt: number;
    finished: boolean;
  } | null>(null);
  const { startSummaryPolling, stopSummaryPolling } = useSidebar();

  const getSummaryStatusMessage = useCallback((status: SummaryStatus) => {
    switch (status) {
      case 'processing':
        return 'Processing transcript...';
      case 'summarizing':
        return 'Generating summary...';
      case 'regenerating':
        return 'Regenerating summary...';
      case 'completed':
        return 'Summary completed';
      case 'error':
        return 'Error generating summary';
      default:
        return '';
    }
  }, []);

  const finishGeneration = useCallback(async (
    generationId: number,
    outcome: 'ok' | 'fallback' | 'generation_error' | 'empty_result' | 'cancelled'
  ) => {
    const attempt = trackedAttemptRef.current;
    if (!attempt || attempt.generationId !== generationId || attempt.finished) {
      return;
    }
    attempt.finished = true;
    await Analytics.trackSummaryGenerationCompleted(
      modelConfig.provider,
      modelConfig.model,
      outcome === 'ok' || outcome === 'fallback',
      (Date.now() - attempt.startedAt) / 1000,
      outcome === 'ok' ? undefined : outcome,
    );
  }, [modelConfig]);

  const processSummary = useCallback(async ({
    transcriptText,
    transcriptTexts,
    customPrompt = '',
    isRegeneration = false,
  }: {
    transcriptText: string;
    transcriptTexts?: string[];
    customPrompt?: string;
    isRegeneration?: boolean;
  }) => {
    const previousAttempt = trackedAttemptRef.current;
    if (previousAttempt && !previousAttempt.finished) {
      await finishGeneration(previousAttempt.generationId, 'cancelled');
    }

    const generationId = ++generationIdRef.current;
    activeProcessIdRef.current = null;
    setSummaryStatus(isRegeneration ? 'regenerating' : 'processing');
    setSummaryError(null);
    const fail = async (
      message: string,
      outcome: 'generation_error' | 'empty_result' = 'generation_error',
    ) => {
      if (generationId !== generationIdRef.current) {
        return;
      }
      activeProcessIdRef.current = null;
      setSummaryError(message);
      setSummaryStatus('error');
      toast.error(`Failed to ${isRegeneration ? 'regenerate' : 'generate'} summary`, {
        description: message,
      });
      await finishGeneration(generationId, outcome);
    };

    try {
      if (!transcriptText.trim()) {
        await fail('No transcript text available. Please add some text first.', 'empty_result');
        return;
      }

      const timeSinceRecording = (Date.now() - new Date(meeting.created_at).getTime()) / 60000;
      trackedAttemptRef.current = {
        generationId,
        startedAt: Date.now(),
        finished: false,
      };
      await Analytics.trackSummaryGenerationStarted(
        modelConfig.provider,
        modelConfig.model,
        transcriptText.length,
        timeSinceRecording,
      );
      if (customPrompt.trim()) {
        await Analytics.trackCustomPromptUsed(customPrompt.trim().length);
      }
      toast.info(`${isRegeneration ? 'Regenerating' : 'Generating'} summary...`, {
        description: `Using ${modelConfig.provider}/${modelConfig.model}`,
        duration: 3000,
      });

      const summaryLanguage = await resolveSummaryLanguage(
        meeting.id,
        transcriptTexts?.length ? transcriptTexts : [transcriptText],
      );
      if (generationId !== generationIdRef.current) {
        return;
      }

      const result = await invokeTauri<ProcessTranscriptResponse>('api_process_transcript', {
        text: transcriptText,
        model: modelConfig.provider,
        modelName: modelConfig.model,
        meetingId: meeting.id,
        chunkSize: 40000,
        overlap: 1000,
        customPrompt,
        templateId: selectedTemplate,
        summaryLanguage,
      });
      const processId = result.process_id;
      if (generationId !== generationIdRef.current) {
        await invokeTauri('api_cancel_summary', { meetingId: meeting.id, processId }).catch(() => {});
        return;
      }
      activeProcessIdRef.current = processId;

      startSummaryPolling(meeting.id, processId, async (pollingResult) => {
        if (generationId !== generationIdRef.current) {
          return;
        }
        if (pollingResult.status === 'cancelled') {
          const existing = await invokeTauri<SummaryProcessResponse>('api_get_summary', {
            meetingId: meeting.id,
          });
          if (generationId !== generationIdRef.current) {
            return;
          }
          const restoredSummary = parseSummaryContent(existing.data);
          setAiSummary(restoredSummary);
          setSummaryStatus(restoredSummary ? 'completed' : 'idle');
          setSummaryError(null);
          activeProcessIdRef.current = null;
          await finishGeneration(generationId, 'cancelled');
          return;
        }

        if (pollingResult.status === 'error' || pollingResult.status === 'failed') {
          const errorMessage = pollingResult.error
            || `Summary ${isRegeneration ? 'regeneration' : 'generation'} failed`;
          if (isRegeneration) {
            const existing = await invokeTauri<SummaryProcessResponse>('api_get_summary', {
              meetingId: meeting.id,
            });
            if (generationId !== generationIdRef.current) {
              return;
            }
            const restoredSummary = parseSummaryContent(existing.data);
            if (restoredSummary) {
              setAiSummary(restoredSummary);
              setSummaryStatus('completed');
              setSummaryError(null);
              toast.error('Failed to regenerate summary', {
                description: `${errorMessage}. Your previous summary has been restored.`,
              });
              activeProcessIdRef.current = null;
              await finishGeneration(generationId, 'generation_error');
              return;
            }
          }
          await fail(errorMessage);
          return;
        }

        if (pollingResult.status === 'completed') {
          const summary = parseSummaryContent(pollingResult.data);
          if (!summary) {
            await fail(
              'Summary generation completed without visible content. Please retry.',
              'empty_result',
            );
            return;
          }
          const metadata = readSummaryMetadata(pollingResult.data);
          const meetingName = metadata.meetingName || pollingResult.meetingName;
          if (meetingName) {
            updateMeetingTitle(meetingName);
          }
          setAiSummary(summary);
          setSummaryStatus('completed');
          activeProcessIdRef.current = null;
          setSummaryError(null);
          if (metadata.normalizationFallback) {
            toast.warning('Summary generated with fallback', {
              description: 'English normalization failed, so the original sanitized summary was kept.',
            });
          } else {
            toast.success('Summary generated successfully!', {
              description: metadata.reasoningStripped
                ? 'Your meeting summary is ready. Model reasoning was filtered out of the notes.'
                : 'Your meeting summary is ready',
              duration: 4000,
            });
          }
          if (meetingName && onMeetingUpdated) {
            await onMeetingUpdated();
          }
          await finishGeneration(
            generationId,
            metadata.normalizationFallback ? 'fallback' : 'ok',
          );
        }
      });
    } catch (error) {
      await fail(error instanceof Error ? error.message : 'Summary generation failed.');
    }
  }, [
    finishGeneration,
    meeting.created_at,
    meeting.id,
    modelConfig,
    onMeetingUpdated,
    selectedTemplate,
    setAiSummary,
    startSummaryPolling,
    updateMeetingTitle,
  ]);

  // Helper function to fetch ALL transcripts for summary generation
  const fetchAllTranscripts = useCallback(async (meetingId: string): Promise<Transcript[]> => {
    try {
      console.log('📊 Fetching all transcripts for meeting:', meetingId);

      // First, get total count by fetching first page
      const firstPage = await invokeTauri('api_get_meeting_transcripts', {
        meetingId,
        limit: 1,
        offset: 0,
      }) as { transcripts: Transcript[]; total_count: number; has_more: boolean };

      const totalCount = firstPage.total_count;
      console.log(`📊 Total transcripts in database: ${totalCount}`);

      if (totalCount === 0) {
        return [];
      }

      // Fetch all transcripts in one call
      const allData = await invokeTauri('api_get_meeting_transcripts', {
        meetingId,
        limit: totalCount,
        offset: 0,
      }) as { transcripts: Transcript[]; total_count: number; has_more: boolean };

      console.log(`✅ Fetched ${allData.transcripts.length} transcripts from database`);
      return allData.transcripts;
    } catch (error) {
      console.error('❌ Error fetching all transcripts:', error);
      toast.error('Failed to fetch transcripts for summary generation');
      return [];
    }
  }, []);

  const buildSummaryTranscriptPayload = useCallback((allTranscripts: Transcript[]) => {
    const formatTime = (seconds: number | undefined, fallbackTimestamp: string): string => {
      if (seconds === undefined) {
        return fallbackTimestamp;
      }
      const totalSecs = Math.floor(seconds);
      return `[${Math.floor(totalSecs / 60).toString().padStart(2, '0')}:${(totalSecs % 60).toString().padStart(2, '0')}]`;
    };

    return {
      transcriptText: allTranscripts
        .map((transcript) => `${formatTime(transcript.audio_start_time, transcript.timestamp)} ${transcript.text}`)
        .join('\n'),
      transcriptTexts: allTranscripts.map((transcript) => transcript.text),
    };
  }, []);

  const showPreflightError = useCallback((message: string) => {
    setSummaryError(message);
    setSummaryStatus('error');
    toast.error(message);
  }, []);

  const handleGenerateSummary = useCallback(async (customPrompt: string = '') => {
    if (isModelConfigLoading) {
      toast.info('Loading model configuration, please wait...');
      return;
    }
    const allTranscripts = await fetchAllTranscripts(meeting.id);
    if (!allTranscripts.length) {
      showPreflightError('No transcripts available for summary');
      return;
    }

    try {
      if (modelConfig.provider === 'ollama') {
        const models = await invokeTauri<unknown[]>('get_ollama_models', {
          endpoint: modelConfig.ollamaEndpoint || null,
        });
        if (models.length === 0) {
          showPreflightError('No Ollama models found. Please download gemma3:1b from Model Settings.');
          return;
        }
      }
      if (modelConfig.provider === 'builtin-ai') {
        if (!modelConfig.model) {
          showPreflightError('No built-in AI model selected. Please select a model in settings.');
          onOpenModelSettings?.();
          return;
        }
        const isReady = await invokeTauri<boolean>('builtin_ai_is_model_ready', {
          modelName: modelConfig.model,
          refresh: true,
        });
        if (!isReady) {
          showPreflightError('Built-in AI model is not ready. Please check model settings.');
          onOpenModelSettings?.();
          return;
        }
      }
    } catch (error) {
      console.error('Failed to validate summary model:', error);
      showPreflightError('Failed to validate summary model. Please check model settings.');
      return;
    }

    await processSummary({
      ...buildSummaryTranscriptPayload(allTranscripts),
      customPrompt,
    });
  }, [
    buildSummaryTranscriptPayload,
    fetchAllTranscripts,
    isModelConfigLoading,
    meeting.id,
    modelConfig,
    onOpenModelSettings,
    processSummary,
    showPreflightError,
  ]);

  // Public API: Regenerate summary from the current saved transcript
  const handleRegenerateSummary = useCallback(async () => {
    const allTranscripts = await fetchAllTranscripts(meeting.id);

    if (!allTranscripts.length) {
      console.error('No transcripts available for regeneration');
      toast.error('No transcripts available for summary regeneration');
      return;
    }

    await processSummary({
      ...buildSummaryTranscriptPayload(allTranscripts),
      isRegeneration: true
    });
  }, [meeting.id, fetchAllTranscripts, buildSummaryTranscriptPayload, processSummary]);

  // Public API: Stop ongoing summary generation
  const handleStopGeneration = useCallback(async () => {
    const generationId = generationIdRef.current;
    const processId = activeProcessIdRef.current;
    generationIdRef.current += 1;
    activeProcessIdRef.current = null;
    if (processId) {
      try {
        await invokeTauri('api_cancel_summary', {
          meetingId: meeting.id,
          processId,
        });
      } catch (error) {
        console.error('Failed to cancel summary generation:', error);
      }
      stopSummaryPolling(meeting.id, processId);
    }
    setSummaryStatus('idle');
    setSummaryError(null);
    await finishGeneration(generationId, 'cancelled');
    toast.info('Summary generation stopped', {
      description: 'You can generate a new summary anytime',
      duration: 3000,
    });
  }, [finishGeneration, meeting.id, stopSummaryPolling]);

  return {
    summaryStatus,
    summaryError,
    handleGenerateSummary,
    handleRegenerateSummary,
    handleStopGeneration,
    getSummaryStatusMessage,
  };
}
