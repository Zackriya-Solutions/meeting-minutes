import { useState, useCallback, useRef } from 'react';
import { Transcript, Summary } from '@/types';
import { BlockNoteSummaryViewRef } from '@/components/AISummary/BlockNoteSummaryView';
import { CurrentMeeting, useSidebar } from '@/components/Sidebar/SidebarProvider';
import { invoke as invokeTauri } from '@tauri-apps/api/core';
import { toast } from 'sonner';

interface UseMeetingDataProps {
  meeting: any;
  summaryData: Summary | null;
  onMeetingUpdated?: () => Promise<void>;
}

export function useMeetingData({ meeting, summaryData, onMeetingUpdated }: UseMeetingDataProps) {
  // State
  const [transcripts] = useState<Transcript[]>(meeting.transcripts);
  const [meetingTitle, setMeetingTitle] = useState(meeting.title || '+ New Call');
  const [isEditingTitle, setIsEditingTitle] = useState(false);
  const [isTitleDirty, setIsTitleDirty] = useState(false);
  const [aiSummary, setAiSummary] = useState<Summary | null>(summaryData);
  const [isSaving, setIsSaving] = useState(false);
  const [, setIsSummaryDirty] = useState(false);
  const [, setError] = useState<string>('');

  // Ref for BlockNoteSummaryView
  const blockNoteSummaryRef = useRef<BlockNoteSummaryViewRef>(null);

  // Sidebar context
  const { setCurrentMeeting, setMeetings, meetings: sidebarMeetings } = useSidebar();

  // Handlers
  const handleTitleChange = useCallback((newTitle: string) => {
    setMeetingTitle(newTitle);
    setIsTitleDirty(true);
  }, []);

  const handleSummaryChange = useCallback((newSummary: Summary) => {
    setAiSummary(newSummary);
  }, []);

  const handleSaveMeetingTitle = useCallback(async () => {
    try {
      await invokeTauri('api_save_meeting_title', {
        meetingId: meeting.id,
        title: meetingTitle,
      });

      console.log('Save meeting title success');
      setIsTitleDirty(false);

      // Update meetings with new title
      const updatedMeetings = sidebarMeetings.map((m: CurrentMeeting) =>
        m.id === meeting.id ? { id: m.id, title: meetingTitle } : m
      );
      setMeetings(updatedMeetings);
      setCurrentMeeting({ id: meeting.id, title: meetingTitle });
      return true;
    } catch (error) {
      console.error('Failed to save meeting title:', error);
      if (error instanceof Error) {
        setError(error.message);
      } else {
        setError('Failed to save meeting title: Unknown error');
      }
      return false;
    }
  }, [meeting.id, meetingTitle, sidebarMeetings, setMeetings, setCurrentMeeting]);

  const handleSaveSummary = useCallback(async (summary: Summary | { markdown?: string; summary_json?: any[] }) => {
    console.log('📄 handleSaveSummary called with:', {
      hasMarkdown: 'markdown' in summary,
      hasSummaryJson: 'summary_json' in summary,
      summaryKeys: Object.keys(summary)
    });

    try {
      let formattedSummary: any;

      // Check if it's the new BlockNote format
      if ('markdown' in summary || 'summary_json' in summary) {
        console.log('📄 Saving new format (markdown/blocknote)');
        formattedSummary = summary;
      } else {
        console.log('📄 Saving legacy format');
        formattedSummary = {
          MeetingName: meetingTitle,
          MeetingNotes: {
            sections: Object.entries(summary).map(([, section]) => ({
              title: section.title,
              blocks: section.blocks
            }))
          }
        };
      }

      await invokeTauri('api_save_meeting_summary', {
        meetingId: meeting.id,
        summary: formattedSummary,
      });

      console.log('✅ Save meeting summary success');
    } catch (error) {
      console.error('❌ Failed to save meeting summary:', error);
      if (error instanceof Error) {
        setError(error.message);
      } else {
        setError('Failed to save meeting summary: Unknown error');
      }
    }
  }, [meeting.id, meetingTitle]);

  const saveAllChanges = useCallback(async () => {
    setIsSaving(true);
    try {
      // Save meeting title only if changed
      if (isTitleDirty) {
        await handleSaveMeetingTitle();
      }

      // Save BlockNote editor changes if dirty
      if (blockNoteSummaryRef.current?.isDirty) {
        console.log('💾 Saving BlockNote editor changes...');
        await blockNoteSummaryRef.current.saveSummary();
      } else if (aiSummary) {
        await handleSaveSummary(aiSummary);
      }

      toast.success("Changes saved successfully");
    } catch (error) {
      console.error('Failed to save changes:', error);
      toast.error("Failed to save changes", { description: String(error) });
    } finally {
      setIsSaving(false);
    }
  }, [isTitleDirty, handleSaveMeetingTitle, aiSummary, handleSaveSummary]);

  // Update meeting title from external source (e.g., AI summary)
  const updateMeetingTitle = useCallback((newTitle: string) => {
    console.log('📝 Updating meeting title to:', newTitle);
    setMeetingTitle(newTitle);
    const updatedMeetings = sidebarMeetings.map((m: CurrentMeeting) =>
      m.id === meeting.id ? { id: m.id, title: newTitle } : m
    );
    setMeetings(updatedMeetings);
    setCurrentMeeting({ id: meeting.id, title: newTitle });
  }, [meeting.id, sidebarMeetings, setMeetings, setCurrentMeeting]);

  return {
    // State
    transcripts,
    meetingTitle,
    isEditingTitle,
    isTitleDirty,
    aiSummary,
    isSaving,
    blockNoteSummaryRef,

    // Setters
    setMeetingTitle,
    setIsEditingTitle,
    setAiSummary,
    setIsSummaryDirty,

    // Handlers
    handleTitleChange,
    handleSummaryChange,
    handleSaveSummary,
    handleSaveMeetingTitle,
    saveAllChanges,
    updateMeetingTitle,
  };
}
