"use client"
import { useSidebar } from "@/components/Sidebar/SidebarProvider";
import { useState, useEffect, useCallback, useRef, Suspense } from "react";
import { Transcript, Summary } from "@/types";
import PageContent from "./page-content";
import { useRouter, useSearchParams } from "next/navigation";
import Analytics from "@/lib/analytics";
import { invoke } from "@tauri-apps/api/core";
import { LoaderIcon } from '@/components/memento/LucideCompat';
import { useConfig } from "@/contexts/ConfigContext";
import { usePaginatedTranscripts } from "@/hooks/usePaginatedTranscripts";
import { useMeetingSpeakers } from "@/hooks/useMeetingSpeakers";
import { useT } from "@/lib/i18n";

interface MeetingDetailsResponse {
  id: string;
  title: string;
  created_at: string;
  updated_at: string;
  transcripts: Transcript[];
  folder_path?: string;
}

type SummaryLoadStatus = 'loading' | 'loaded' | 'absent' | 'error';

function parsePersistedSummary(rawData: unknown): Summary | null {
  let parsedData: any = rawData;
  if (typeof parsedData === 'string') {
    try {
      parsedData = JSON.parse(parsedData);
    } catch {
      return null;
    }
  }
  if (!parsedData || typeof parsedData !== 'object') return null;

  // Current formats must retain their generation/freshness metadata.
  if (parsedData.summary_json || parsedData.markdown) return parsedData as Summary;

  // Legacy section format.
  const { MeetingName: _meetingName, _section_order, ...restSummaryData } = parsedData;
  const formattedSummary: Summary = {};
  const sectionKeys = Array.isArray(_section_order)
    ? _section_order
    : Object.keys(restSummaryData);

  for (const key of sectionKeys) {
    const section = restSummaryData[key];
    if (!section || typeof section !== 'object' || !('title' in section) || !('blocks' in section)) {
      continue;
    }
    const blocks = Array.isArray(section.blocks) ? section.blocks : [];
    formattedSummary[key] = {
      title: typeof section.title === 'string' ? section.title : key,
      blocks: blocks.map((block: any) => ({
        ...block,
        color: 'default',
        content: block?.content?.trim() || '',
      })),
    };
  }

  return Object.keys(formattedSummary).length > 0 ? formattedSummary : null;
}

function MeetingDetailsContent() {
  const searchParams = useSearchParams();
  const meetingId = searchParams.get('id');
  const source = searchParams.get('source'); // Check if navigated from recording
  // Optional jump-to-timestamp deep link (?t=<seconds>) from search results / RAG citations.
  const seekParam = searchParams.get('t');
  const seekToSeconds =
    seekParam != null && seekParam !== '' && Number.isFinite(Number(seekParam)) ? Number(seekParam) : null;
  const { setCurrentMeeting, refetchMeetings, stopSummaryPolling } = useSidebar();
  const { isAutoSummary } = useConfig(); // Get auto-summary toggle state
  const router = useRouter();
  const t = useT();
  const [meetingDetails, setMeetingDetails] = useState<MeetingDetailsResponse | null>(null);
  const [meetingSummary, setMeetingSummary] = useState<Summary | null>(null);
  const meetingSummaryRef = useRef<Summary | null>(null);
  const summaryLoadRequestRef = useRef(0);
  const [summaryLoadStatus, setSummaryLoadStatus] = useState<SummaryLoadStatus>('loading');
  const [summaryLoadError, setSummaryLoadError] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState<boolean>(true);
  const [shouldAutoGenerate, setShouldAutoGenerate] = useState<boolean>(false);
  const [hasCheckedAutoGen, setHasCheckedAutoGen] = useState<boolean>(false);

  // Use pagination hook for efficient transcript loading
  const {
    metadata,
    segments,
    transcripts,
    isLoading: isLoadingTranscripts,
    isLoadingMore,
    hasMore,
    totalCount,
    loadedCount,
    loadMore,
    refetch,
    error: transcriptError,
  } = usePaginatedTranscripts({ meetingId: meetingId || '' });

  // Diarized speaker identities for this meeting. Kept here (alongside the
  // transcript pagination hook) so the `diarization-complete` listener can
  // refresh both speakers and transcripts for the currently open meeting.
  const { speakers, speakersById, refetchSpeakers, renameSpeaker } = useMeetingSpeakers({
    meetingId: meetingId || null,
    onDiarized: refetch,
  });

  const commitMeetingSummary = useCallback((summary: Summary | null) => {
    meetingSummaryRef.current = summary;
    setMeetingSummary(summary);
    setSummaryLoadStatus(summary ? 'loaded' : 'absent');
    setSummaryLoadError(null);
  }, []);

  // Explicit refresh after a Detect action completes (immediate feedback;
  // the diarization-complete event covers refreshes triggered elsewhere).
  const handleSpeakersDetected = useCallback(async () => {
    await refetchSpeakers();
    await refetch();
  }, [refetchSpeakers, refetch]);

  const handleRenameSpeaker = useCallback(async (speakerId: number, displayName: string) => {
    await renameSpeaker(speakerId, displayName);
    // A persisted summary is editable prose, not a live view over speaker ids. Mark it stale
    // immediately; api_get_summary independently verifies the persisted generation snapshot on
    // the next load. We deliberately do not rewrite or regenerate over manual edits silently.
    setMeetingSummary((previous) => {
      if (!previous) return previous;
      const next = {
        ...(previous as any),
        summary_freshness: {
          ...((previous as any).summary_freshness || {}),
          speaker_attribution_status: 'stale',
          speaker_attribution_stale: true,
        },
      } as any;
      meetingSummaryRef.current = next;
      return next;
    });
  }, [renameSpeaker]);

  // Check if gemma3:1b model is available in Ollama
  const checkForGemmaModel = useCallback(async (): Promise<boolean> => {
    try {
      const models = await invoke('get_ollama_models', { endpoint: null }) as any[];
      const hasGemma = models.some((m: any) => m.name === 'gemma3:1b');
      console.log('🔍 Checked for gemma3:1b:', hasGemma);
      return hasGemma;
    } catch (error) {
      console.error('❌ Failed to check Ollama models:', error);
      return false;
    }
  }, []);

  // Set up auto-generation - respects DB as source of truth
  const setupAutoGeneration = useCallback(async () => {
    if (hasCheckedAutoGen) return; // Only check once

    // Only auto-generate if navigated from recording
    if (source !== 'recording') {
      console.log('Not from recording navigation, skipping auto-generation');
      setHasCheckedAutoGen(true);
      return;
    }

    // Respect user's auto-summary toggle preference
    if (!isAutoSummary) {
      console.log('Auto-summary is disabled in settings');
      setHasCheckedAutoGen(true);
      return;
    }

    try {
      // Check what's currently in database
      const currentConfig = await invoke('api_get_model_config') as any;

      // If DB already has a model, use it (never override!)
      if (currentConfig && currentConfig.model) {
        console.log('Using existing model from DB:', currentConfig.model);
        setShouldAutoGenerate(true);
        setHasCheckedAutoGen(true);
        return;
      }

      // DB is empty - check if gemma3:1b exists as fallback
      const hasGemma = await checkForGemmaModel();

      if (hasGemma) {
        console.log('💾 DB empty, using gemma3:1b as initial default');

        await invoke('api_save_model_config', {
          provider: 'ollama',
          model: '',
          whisperModel: 'large-v3',
          apiKey: null,
          ollamaEndpoint: null,
        });

        setShouldAutoGenerate(true);
      } else {
        console.log('⚠️ No model configured and gemma3:1b not found');
      }
    } catch (error) {
      console.error('❌ Failed to setup auto-generation:', error);
    }

    setHasCheckedAutoGen(true);
  }, [hasCheckedAutoGen, checkForGemmaModel, source, isAutoSummary]);

  // Sync meeting metadata from pagination hook to meeting details state
  useEffect(() => {
    if (metadata && (!meetingId || meetingId === 'intro-call')) {
      // If invalid meeting ID, don't sync
      return;
    }

    if (metadata) {
      console.log('Meeting metadata loaded:', metadata);

      // Build meeting details from metadata and paginated transcripts
      setMeetingDetails({
        id: metadata.id,
        title: metadata.title,
        created_at: metadata.created_at,
        updated_at: metadata.updated_at,
        transcripts: transcripts, // Paginated transcripts from hook
        folder_path: metadata.folder_path, // For retranscription feature
      });

      // Sync with sidebar context
      setCurrentMeeting({ id: metadata.id, title: metadata.title });
    }
  }, [metadata, transcripts, meetingId, setCurrentMeeting]);

  // Handle transcript loading errors
  useEffect(() => {
    if (transcriptError) {
      console.error('Error loading transcripts:', transcriptError);
      setError(transcriptError);
    }
  }, [transcriptError]);

  // Extract fetchMeetingDetails for use in child components (now refetches via hook)
  const fetchMeetingDetails = useCallback(async () => {
    if (!meetingId || meetingId === 'intro-call') {
      return;
    }

    // The usePaginatedTranscripts hook automatically refetches when meetingId changes
    // This function is kept for compatibility with onMeetingUpdated callback
    console.log('fetchMeetingDetails called - pagination hook will handle refetch');
  }, [meetingId]);

  const fetchMeetingSummary = useCallback(async (showPageLoader = false) => {
    if (!meetingId || meetingId === 'intro-call') return;

    const requestId = ++summaryLoadRequestRef.current;
    if (showPageLoader) setIsLoading(true);
    setSummaryLoadStatus('loading');
    setSummaryLoadError(null);

    let lastError: unknown = null;
    try {
      // SQLite reads should normally be instant, but background migrations/jobs can produce a
      // transient failure. Retry before presenting an error and never replace known content.
      for (let attempt = 0; attempt < 3; attempt += 1) {
        try {
          const response = await invoke('api_get_summary', { meetingId }) as any;
          if (summaryLoadRequestRef.current !== requestId) return;

          if (response?.data != null) {
            const parsed = parsePersistedSummary(response.data);
            if (!parsed) {
              throw new Error(t('The saved summary has an unsupported or damaged format.'));
            }
            commitMeetingSummary(parsed);
            return;
          }

          if (response?.status === 'idle') {
            if (meetingSummaryRef.current) {
              // A late/stale `idle` response must not erase content already shown in this
              // session. Keep it and surface the inconsistency as recoverable load trouble.
              setSummaryLoadStatus('loaded');
              setSummaryLoadError(t('Could not verify the saved summary. The last loaded version is still shown.'));
            } else {
              commitMeetingSummary(null);
            }
            return;
          }

          if (['pending', 'processing', 'summarizing', 'regenerating'].includes(response?.status)) {
            setSummaryLoadStatus(meetingSummaryRef.current ? 'loaded' : 'loading');
            return;
          }

          throw new Error(response?.error || t('The saved summary could not be loaded.'));
        } catch (error) {
          lastError = error;
          if (attempt < 2) {
            await new Promise((resolve) => window.setTimeout(resolve, 150 * (attempt + 1)));
          }
        }
      }

      if (summaryLoadRequestRef.current !== requestId) return;
      console.error('FETCH SUMMARY: exhausted retries:', lastError);
      setSummaryLoadStatus(meetingSummaryRef.current ? 'loaded' : 'error');
      setSummaryLoadError(
        lastError instanceof Error ? lastError.message : t('The saved summary could not be loaded.')
      );
    } finally {
      if (summaryLoadRequestRef.current === requestId) setIsLoading(false);
    }
  }, [meetingId, commitMeetingSummary, t]);

  // Reset states when meetingId changes (prevent race conditions)
  useEffect(() => {
    setMeetingDetails(null);
    setMeetingSummary(null);
    meetingSummaryRef.current = null;
    setSummaryLoadStatus('loading');
    setSummaryLoadError(null);
    setError(null);
    setIsLoading(true);
    // Reset auto-generation state to allow new meeting to be checked
    setHasCheckedAutoGen(false);
    setShouldAutoGenerate(false);
  }, [meetingId]);

  // Cleanup: Stop polling when navigating away from a meeting
  useEffect(() => {
    return () => {
      if (meetingId) {
        console.log('Cleaning up: Stopping summary polling for meeting:', meetingId);
        stopSummaryPolling(meetingId);
      }
    };
  }, [meetingId, stopSummaryPolling]);

  useEffect(() => {
    console.log('MeetingDetails useEffect triggered - meetingId:', meetingId);

    if (!meetingId || meetingId === 'intro-call') {
      console.warn('No valid meeting ID in URL - meetingId:', meetingId);
      setError(t('No meeting selected'));
      setIsLoading(false);
      Analytics.trackPageView('meeting_details');
      return;
    }

    console.log('Valid meeting ID found, fetching details for:', meetingId);
    void fetchMeetingSummary(true);

    return () => {
      // Ignore a late response from a meeting that is no longer open.
      summaryLoadRequestRef.current += 1;
    };
  }, [meetingId, fetchMeetingSummary, t]);

  // Auto-generation check: runs when meeting is loaded with no summary
  useEffect(() => {
    const checkAutoGen = async () => {
      // Only auto-generate if:
      // 1. We have meeting details
      // 2. No summary exists
      // 3. Meeting has transcripts
      // 4. Haven't checked yet
      if (
        meetingDetails &&
        meetingSummary === null &&
        summaryLoadStatus === 'absent' &&
        meetingDetails.transcripts &&
        meetingDetails.transcripts.length > 0 &&
        !hasCheckedAutoGen
      ) {
        console.log('No summary found, checking for auto-generation...');
        await setupAutoGeneration();
      }
    };

    checkAutoGen();
  }, [meetingDetails, meetingSummary, summaryLoadStatus, hasCheckedAutoGen, setupAutoGeneration]);

  if (error) {
    return (
      <div className="flex items-center justify-center h-screen">
        <div className="text-center">
          <p className="text-[var(--danger)] mb-4">{error}</p>
          <button
            onClick={() => router.push('/')}
            className="px-4 py-2 bg-[var(--gold)] text-[var(--fg-inverse)] rounded hover:bg-[var(--gold-active)]"
          >
            {t('Go Back')}
          </button>
        </div>
      </div>
    );
  }

  // Show loading spinner while initial data loads
  if ((isLoading || isLoadingTranscripts) || !meetingDetails) {
    return <div className="flex items-center justify-center h-screen">
      <LoaderIcon className="animate-spin size-6 " />
    </div>;
  }

  return <PageContent
    meeting={meetingDetails}
    summaryData={meetingSummary}
    summaryLoadStatus={summaryLoadStatus}
    summaryLoadError={summaryLoadError}
    onRetrySummary={() => fetchMeetingSummary(false)}
    onSummaryDataChange={commitMeetingSummary}
    shouldAutoGenerate={shouldAutoGenerate}
    onAutoGenerateComplete={() => setShouldAutoGenerate(false)}
    onMeetingUpdated={async () => {
      // Refetch meeting details to get updated title from backend
      await fetchMeetingDetails();
      // Refetch meetings list to update sidebar
      await refetchMeetings();
    }}
    onRefetchTranscripts={refetch}
    // Pagination props for efficient transcript loading
    segments={segments}
    hasMore={hasMore}
    isLoadingMore={isLoadingMore}
    totalCount={totalCount}
    loadedCount={loadedCount}
    onLoadMore={loadMore}
    seekToSeconds={seekToSeconds}
    // Speaker diarization props
    speakersById={speakersById}
    speakerCount={speakers.length}
    onRenameSpeaker={handleRenameSpeaker}
    onSpeakersDetected={handleSpeakersDetected}
  />;
}

export default function MeetingDetails() {
  return (
    <Suspense fallback={
      <div className="flex items-center justify-center h-screen">
        <LoaderIcon className="animate-spin size-6" />
      </div>
    }>
      <MeetingDetailsContent />
    </Suspense>
  );
}
