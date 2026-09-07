"use client"
import { useSidebar } from "@/components/Sidebar/SidebarProvider";
import { useState, useEffect, useCallback, Suspense } from "react";
import { MeetingSummary, SummaryProcessResponse, Transcript } from "@/types";
import PageContent from "./page-content";
import { useRouter, useSearchParams } from "next/navigation";
import Analytics from "@/lib/analytics";
import { invoke } from "@tauri-apps/api/core";
import { LoaderIcon } from "lucide-react";
import { useConfig } from "@/contexts/ConfigContext";
import { usePaginatedTranscripts } from "@/hooks/usePaginatedTranscripts";
import { parseSummaryContent } from "@/lib/summary-content";

interface MeetingDetailsResponse {
  id: string;
  title: string;
  created_at: string;
  updated_at: string;
  transcripts: Transcript[];
  folder_path?: string;
}

function MeetingDetailsContent() {
  const searchParams = useSearchParams();
  const meetingId = searchParams.get('id');
  const source = searchParams.get('source'); // Check if navigated from recording
  const { setCurrentMeeting, refetchMeetings } = useSidebar();
  const { isAutoSummary } = useConfig(); // Get auto-summary toggle state
  const router = useRouter();
  const [meetingDetails, setMeetingDetails] = useState<MeetingDetailsResponse | null>(null);
  const [summaryResponse, setSummaryResponse] = useState<SummaryProcessResponse | null>(null);
  const [meetingSummary, setMeetingSummary] = useState<MeetingSummary | null>(null);
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
          model: 'gemma3:1b',
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

  // Reset states when meetingId changes (prevent race conditions)
  useEffect(() => {
    setMeetingDetails(null);
    setMeetingSummary(null);
    setSummaryResponse(null);
    setError(null);
    setIsLoading(true);
    // Reset auto-generation state to allow new meeting to be checked
    setHasCheckedAutoGen(false);
    setShouldAutoGenerate(false);
  }, [meetingId]);

  useEffect(() => {
    console.log('MeetingDetails useEffect triggered - meetingId:', meetingId);

    if (!meetingId || meetingId === 'intro-call') {
      console.warn('No valid meeting ID in URL - meetingId:', meetingId);
      setError("No meeting selected");
      setIsLoading(false);
      Analytics.trackPageView('meeting_details');
      return;
    }

    console.log('Valid meeting ID found, fetching details for:', meetingId);

    setMeetingDetails(null);
    setMeetingSummary(null);
    setSummaryResponse(null);
    setError(null);
    setIsLoading(true);

    let cancelled = false;
    const fetchMeetingSummary = async () => {
      try {
        const response = await invoke<SummaryProcessResponse>('api_get_summary', {
          meetingId,
        });
        if (cancelled) return;
        setSummaryResponse(response);
        const summary = parseSummaryContent(response.data);
        setMeetingSummary(response.status === 'idle' ? null : summary);
      } catch (error) {
        if (cancelled) return;
        console.error('FETCH SUMMARY: Error fetching meeting summary:', error);
        setMeetingSummary(null);
      }
    };

    const loadData = async () => {
      try {
        await fetchMeetingSummary();
      } finally {
        if (!cancelled) setIsLoading(false);
      }
    };

    loadData();
    return () => { cancelled = true; };
  }, [meetingId]);

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
        !isLoading &&
        summaryResponse?.status === 'idle' &&
        meetingSummary === null &&
        meetingDetails.transcripts &&
        meetingDetails.transcripts.length > 0 &&
        !hasCheckedAutoGen
      ) {
        console.log('No summary found, checking for auto-generation...');
        await setupAutoGeneration();
      }
    };

    checkAutoGen();
  }, [meetingDetails, meetingSummary, summaryResponse, isLoading, hasCheckedAutoGen, setupAutoGeneration]);

  if (error) {
    return (
      <div className="flex items-center justify-center h-screen">
        <div className="text-center">
          <p className="text-red-500 mb-4">{error}</p>
          <button
            onClick={() => router.push('/')}
            className="px-4 py-2 bg-blue-500 text-white rounded hover:bg-blue-600"
          >
            Go Back
          </button>
        </div>
      </div>
    );
  }

  // Show loading spinner while initial data loads
  if ((isLoading || isLoadingTranscripts) || !meetingDetails || meetingDetails.id !== meetingId) {
    return <div className="flex items-center justify-center h-screen">
      <LoaderIcon className="animate-spin size-6 " />
    </div>;
  }

  return <PageContent
    key={meetingId}
    initialSummary={summaryResponse}
    meeting={meetingDetails}
    summaryData={meetingSummary}
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
