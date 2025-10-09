"use client"
import { useSidebar } from "@/components/Sidebar/SidebarProvider";
import { useState, useEffect, useCallback, Suspense } from "react";
import { Transcript, Summary } from "@/types";
import PageContent from "./page-content";
import { useRouter, useSearchParams } from "next/navigation";
import Analytics from "@/lib/analytics";
import { invoke } from "@tauri-apps/api/core";
import { LoaderIcon } from "lucide-react";

interface MeetingDetailsResponse {
  id: string;
  title: string;
  created_at: string;
  updated_at: string;
  transcripts: Transcript[];
}

const sampleSummary: Summary = {
  key_points: { title: "Key Points", blocks: [] },
  action_items: { title: "Action Items", blocks: [] },
  decisions: { title: "Decisions", blocks: [] },
  main_topics: { title: "Main Topics", blocks: [] }
};

function MeetingDetailsContent() {
  const searchParams = useSearchParams();
  const meetingId = searchParams.get('id');
  const { setCurrentMeeting, refetchMeetings } = useSidebar();
  const router = useRouter();
  const [meetingDetails, setMeetingDetails] = useState<MeetingDetailsResponse | null>(null);
  const [meetingSummary, setMeetingSummary] = useState<Summary | null>(null);
  const [error, setError] = useState<string | null>(null);

  // Extract fetchMeetingDetails so it can be called from child components
  const fetchMeetingDetails = useCallback(async () => {
    if (!meetingId || meetingId === 'intro-call') {
      return;
    }

    try {
      const data = await invoke('api_get_meeting', {
        meetingId: meetingId,
      }) as any;
      console.log('Meeting details:', data);
      setMeetingDetails(data);

      // Sync with sidebar context
      setCurrentMeeting({ id: data.id, title: data.title });
    } catch (error) {
      console.error('Error fetching meeting details:', error);
      setError("Failed to load meeting details");
    }
  }, [meetingId, setCurrentMeeting]);

  // Reset states when meetingId changes
  useEffect(() => {
    setMeetingDetails(null);
    setMeetingSummary(null);
    setError(null);
  }, [meetingId]);

  useEffect(() => {
    console.log('🔍 MeetingDetails useEffect triggered - meetingId:', meetingId);

    if (!meetingId || meetingId === 'intro-call') {
      console.warn('⚠️ No valid meeting ID in URL - meetingId:', meetingId);
      setError("No meeting selected");
      Analytics.trackPageView('meeting_details');
      return;
    }

    console.log('✅ Valid meeting ID found, fetching details for:', meetingId);

    setMeetingDetails(null);
    setMeetingSummary(null);
    setError(null);

    const fetchMeetingSummary = async () => {
      try {
        const summary = await invoke('api_get_summary', {
          meetingId: meetingId,
        }) as any;

        console.log('🔍 FETCH SUMMARY: Raw response:', summary);

        // Check if the summary request failed with 404 or error status, or if no summary exists yet (idle)
        if (summary.status === 'error' || summary.error || summary.status === 'idle') {
          console.warn('Meeting summary not found, error occurred, or no summary generated yet:', summary.error || 'idle');
          setMeetingSummary(sampleSummary);
          return;
        }

        const summaryData = summary.data || {};

        // Parse if it's a JSON string (backend may return double-encoded JSON)
        let parsedData = summaryData;
        if (typeof summaryData === 'string') {
          try {
            parsedData = JSON.parse(summaryData);
          } catch (e) {
            parsedData = {};
          }
        }

        console.log('🔍 FETCH SUMMARY: Parsed data:', parsedData);

        // Priority 1: BlockNote JSON format
        if (parsedData.summary_json) {
          setMeetingSummary(parsedData as any);
          return;
        }

        // Priority 2: Markdown format
        if (parsedData.markdown) {
          setMeetingSummary(parsedData as any);
          return;
        }

        // Legacy format - apply formatting
        console.log('📦 LEGACY FORMAT: Detected legacy format, applying section formatting');

        const { MeetingName, _section_order, ...restSummaryData } = parsedData;

        // Format the summary data with consistent styling - PRESERVE ORDER
        const formattedSummary: Summary = {};

        // Use section order if available to maintain exact order and handle duplicates
        const sectionKeys = _section_order || Object.keys(restSummaryData);

        console.log('📦 LEGACY FORMAT: Processing sections:', sectionKeys);

        for (const key of sectionKeys) {
          try {
            const section = restSummaryData[key];
            // Comprehensive null checks to prevent the error
            if (section &&
              typeof section === 'object' &&
              'title' in section &&
              'blocks' in section) {

              const typedSection = section as { title?: string; blocks?: any[] };

              // Ensure blocks is an array before mapping
              if (Array.isArray(typedSection.blocks)) {
                formattedSummary[key] = {
                  title: typedSection.title || key,
                  blocks: typedSection.blocks.map((block: any) => ({
                    ...block,
                    // type: 'bullet',
                    color: 'default',
                    content: block?.content?.trim() || ''
                  }))
                };
              } else {
                // Handle case where blocks is not an array
                console.warn(`📦 LEGACY FORMAT: Section ${key} has invalid blocks:`, typedSection.blocks);
                formattedSummary[key] = {
                  title: typedSection.title || key,
                  blocks: []
                };
              }
            } else {
              console.warn(`📦 LEGACY FORMAT: Skipping invalid section ${key}:`, section);
            }
          } catch (error) {
            console.warn(`📦 LEGACY FORMAT: Error processing section ${key}:`, error);
            // Continue processing other sections
          }
        }

        console.log('📦 LEGACY FORMAT: Formatted summary:', formattedSummary);
        setMeetingSummary(formattedSummary);
      } catch (error) {
        console.error('❌ FETCH SUMMARY: Error fetching meeting summary:', error);
        // Don't set error state for summary fetch failure, just use sample summary
        setMeetingSummary(sampleSummary);
      }
    };

    fetchMeetingDetails();
    fetchMeetingSummary();
  }, [meetingId, fetchMeetingDetails]);

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

  if (!meetingDetails || !meetingSummary) {
    return <div className="flex items-center justify-center h-screen">
      <LoaderIcon className="animate-spin size-6 " />
    </div>;
  }

  return <PageContent
    meeting={meetingDetails}
    summaryData={meetingSummary}
    onMeetingUpdated={async () => {
      // Refetch meeting details to get updated title from backend
      await fetchMeetingDetails();
      // Refetch meetings list to update sidebar
      await refetchMeetings();
    }}
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
