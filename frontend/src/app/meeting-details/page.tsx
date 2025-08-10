"use client"
import { useSidebar } from "@/components/Sidebar/SidebarProvider";
import { useState, useEffect } from "react";
import { Transcript, Summary } from "@/types";
import PageContent from "./page-content";
import { useRouter } from "next/navigation";
import Analytics from "@/lib/analytics";
import { invoke } from "@tauri-apps/api/core";

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

export default function MeetingDetails() {
  const { currentMeeting , serverAddress} = useSidebar();
  const router = useRouter();
  const [meetingDetails, setMeetingDetails] = useState<MeetingDetailsResponse | null>(null);
  const [meetingSummary, setMeetingSummary] = useState<Summary|null>(null);
  const [error, setError] = useState<string | null>(null);

  // Reset states when currentMeeting changes
  useEffect(() => {
    setMeetingDetails(null);
    setMeetingSummary(null);
    setError(null);
  }, [currentMeeting?.id]);

  useEffect(() => {
    if (!currentMeeting?.id || currentMeeting.id === 'intro-call') {
      setError("No meeting selected");
      Analytics.trackPageView('meeting_details');
      return;
    }

    setMeetingDetails(null);
    setMeetingSummary(null);
    setError(null);

    const fetchMeetingDetails = async () => {
      try {
        const data = await invoke('api_get_meeting', {
          meetingId: currentMeeting.id,
        }) as any;
        console.log('Meeting details:', data);
        setMeetingDetails(data);
      } catch (error) {
        console.error('Error fetching meeting details:', error);
        setError("Failed to load meeting details");
      }
    };

    const fetchMeetingSummary = async () => {
      try {
        const summary = await invoke('api_get_summary', {
          meetingId: currentMeeting.id,
        }) as any;
        const summaryData = summary.data || {};
        const { MeetingName, _section_order, ...restSummaryData } = summaryData;
        
        // Format the summary data with consistent styling - PRESERVE ORDER
        const formattedSummary: Summary = {};
        
        // Use section order if available to maintain exact order and handle duplicates
        const sectionKeys = _section_order || Object.keys(restSummaryData);
        
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
                console.warn(`Section ${key} has invalid blocks:`, typedSection.blocks);
                formattedSummary[key] = {
                  title: typedSection.title || key,
                  blocks: []
                };
              }
            } else {
              console.warn(`Skipping invalid section ${key}:`, section);
            }
          } catch (error) {
            console.warn(`Error processing section ${key}:`, error);
            // Continue processing other sections
          }
        }
        setMeetingSummary(formattedSummary);
      } catch (error) {
        console.error('Error fetching meeting summary:', error);
        // Don't set error state for summary fetch failure, just use sample summary
        setMeetingSummary(sampleSummary);
      }
    };

    fetchMeetingDetails();
    fetchMeetingSummary();
  }, [currentMeeting?.id, serverAddress]);

  // if (error) {
  //   return (
  //     <div className="flex items-center justify-center h-screen">
  //       <div className="text-center">
  //         <p className="text-red-500 mb-4">{error}</p>
  //         <button
  //           onClick={() => router.push('/')}
  //           className="px-4 py-2 bg-blue-500 text-white rounded hover:bg-blue-600"
  //         >
  //           Go Back
  //         </button>
  //       </div>
  //     </div>
  //   );
  // }

  if (!meetingDetails || !meetingSummary) {
    return <div className="flex items-center justify-center h-screen">Loading...</div>;
  }

  return <PageContent meeting={meetingDetails} summaryData={meetingSummary} />;
}