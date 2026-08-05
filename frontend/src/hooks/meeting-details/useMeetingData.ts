import { useState, useCallback, useRef, useEffect } from 'react';
import { Transcript, Summary } from '@/types';
import { BlockNoteSummaryViewRef } from '@/components/AISummary/BlockNoteSummaryView';
import { useSidebar } from '@/components/Sidebar/SidebarProvider';
import { invoke as invokeTauri } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import { useLanguage } from '@/lib/i18n';
import { getMeetingDisplayInfo } from '@/lib/meetingDisplay';

interface UseMeetingDataProps {
  meeting: any;
  summaryData: Summary | null;
  onMeetingUpdated?: () => Promise<void>;
}

export function useMeetingData({ meeting, summaryData, onMeetingUpdated }: UseMeetingDataProps) {
  const { t, lang } = useLanguage();
  const initialMeetingTitle = getMeetingDisplayInfo({
    title: meeting.title,
    createdAt: meeting.created_at,
    occurredAt: meeting.occurred_at,
    folderPath: meeting.folder_path,
  }, lang).title;
  // State
  // Use prop directly since summary generation fetches transcripts independently
  const transcripts = meeting.transcripts;
  const [meetingTitle, setMeetingTitle] = useState(initialMeetingTitle);
  const [isEditingTitle, setIsEditingTitle] = useState(false);
  const [isTitleDirty, setIsTitleDirty] = useState(false);
  const [aiSummary, setAiSummary] = useState<Summary | null>(summaryData);
  const [isSaving, setIsSaving] = useState(false);
  const [, setIsSummaryDirty] = useState(false);
  const [, setError] = useState<string>('');

  // Ref for BlockNoteSummaryView
  const blockNoteSummaryRef = useRef<BlockNoteSummaryViewRef>(null);
  const meetingTitleRef = useRef(initialMeetingTitle);
  const savedMeetingTitleRef = useRef(initialMeetingTitle);
  const titleSaveQueueRef = useRef<Promise<void>>(Promise.resolve());
  const savingOperationsRef = useRef(0);

  // Sidebar context
  const { setCurrentMeeting, setMeetings } = useSidebar();

  // Sync aiSummary state when summaryData prop changes (fixes display of fetched summaries)
  useEffect(() => {
    console.log('[useMeetingData] Syncing summary data from prop:', summaryData ? 'present' : 'null');
    setAiSummary(summaryData);
  }, [summaryData]); // Only trigger when parent prop changes, not when aiSummary changes

  // Handlers
  const handleTitleChange = useCallback((newTitle: string) => {
    meetingTitleRef.current = newTitle;
    setMeetingTitle(newTitle);
    setIsTitleDirty(newTitle.trim() !== savedMeetingTitleRef.current);
  }, []);

  const handleSummaryChange = useCallback((newSummary: Summary) => {
    setAiSummary(newSummary);
  }, []);

  const handleSaveMeetingTitle = useCallback(async () => {
    const titleToSave = meetingTitleRef.current.trim();
    if (!titleToSave) {
      throw new Error(t('Meeting title cannot be empty'));
    }

    // Enter followed by blur, or a click on "Save" while the textarea is losing
    // focus, can request the same save twice. Serialize title writes so an older,
    // slower IPC response can never overwrite a newer rename.
    const previousSave = titleSaveQueueRef.current;
    const save = (async () => {
      try {
        await previousSave.catch(() => undefined);
        if (titleToSave === savedMeetingTitleRef.current) {
          if (meetingTitleRef.current.trim() === titleToSave) setIsTitleDirty(false);
          return;
        }

        await invokeTauri('api_save_meeting_title', {
          meetingId: meeting.id,
          title: titleToSave,
        });

        savedMeetingTitleRef.current = titleToSave;
        if (meetingTitleRef.current.trim() === titleToSave) {
          meetingTitleRef.current = titleToSave;
          setMeetingTitle(titleToSave);
          setIsTitleDirty(false);
        }

        // Keep every view over the shared archive in sync immediately. Preserve
        // date, duration, and folder metadata: the home list uses all of it for
        // grouping and display.
        setMeetings((current) => current.map((item) =>
          item.id === meeting.id ? { ...item, title: titleToSave } : item
        ));
        setCurrentMeeting((current) => {
          if (!current || current.id !== meeting.id) return current;
          return { ...current, title: titleToSave };
        });
        try {
          await onMeetingUpdated?.();
        } catch (refreshError) {
          // The database write and shared-state update already succeeded. A
          // follow-up refresh must not turn a successful rename into an error.
          console.warn('Meeting title saved, but archive refresh failed:', refreshError);
        }
      } catch (error) {
        console.error('Failed to save meeting title:', error);
        if (meetingTitleRef.current.trim() === titleToSave) {
          const savedTitle = savedMeetingTitleRef.current;
          meetingTitleRef.current = savedTitle;
          setMeetingTitle(savedTitle);
          setIsTitleDirty(false);
        }
        setError(error instanceof Error ? error.message : 'Failed to save meeting title: Unknown error');
        throw error;
      }
    })();

    titleSaveQueueRef.current = save;
    await save;
  }, [meeting.id, onMeetingUpdated, setMeetings, setCurrentMeeting, t]);

  const beginSaving = useCallback(() => {
    savingOperationsRef.current += 1;
    setIsSaving(true);
  }, []);

  const finishSaving = useCallback(() => {
    savingOperationsRef.current = Math.max(0, savingOperationsRef.current - 1);
    if (savingOperationsRef.current === 0) setIsSaving(false);
  }, []);

  const handleFinishEditTitle = useCallback(async () => {
    setIsEditingTitle(false);
    const normalizedTitle = meetingTitleRef.current.trim();
    if (normalizedTitle === savedMeetingTitleRef.current) {
      meetingTitleRef.current = normalizedTitle;
      setMeetingTitle(normalizedTitle);
      setIsTitleDirty(false);
      return;
    }

    beginSaving();
    try {
      await handleSaveMeetingTitle();
      toast.success(t('Meeting title updated successfully'));
    } catch (error) {
      toast.error(t('Failed to update meeting title'), { description: String(error) });
    } finally {
      finishSaving();
    }
  }, [beginSaving, finishSaving, handleSaveMeetingTitle, t]);

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
      throw error;
    }
  }, [meeting.id, meetingTitle]);

  const saveAllChanges = useCallback(async () => {
    beginSaving();
    try {
      // Save meeting title only if changed
      if (isTitleDirty) {
        await handleSaveMeetingTitle();
      }

      // Save BlockNote editor changes if dirty
      if (blockNoteSummaryRef.current?.isDirty) {
        console.log('💾 Saving BlockNote editor changes...');
        await blockNoteSummaryRef.current.saveSummary();
      }

      toast.success(t("Changes saved successfully"));
    } catch (error) {
      console.error('Failed to save changes:', error);
      toast.error(t("Failed to save changes"), { description: String(error) });
    } finally {
      finishSaving();
    }
  }, [beginSaving, finishSaving, isTitleDirty, handleSaveMeetingTitle, t]);

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
    handleFinishEditTitle,
    handleSaveSummary,
    handleSaveMeetingTitle,
    saveAllChanges,
  };
}
