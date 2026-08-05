import { useState, useCallback, useRef, useEffect } from 'react';
import { Transcript, Summary } from '@/types';
import { BlockNoteSummaryViewRef } from '@/components/AISummary/BlockNoteSummaryView';
import { useSidebar } from '@/components/Sidebar/SidebarProvider';
import { invoke as invokeTauri } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import { useLanguage } from '@/lib/i18n';
import { getMeetingDisplayInfo } from '@/lib/meetingDisplay';
import {
  isLatestMeetingTitle,
  MeetingTitleSaveQueue,
  normalizeMeetingTitle,
} from '@/lib/meetingTitleSaveQueue';

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
  const titleSaveQueueRef = useRef(new MeetingTitleSaveQueue());
  const savingOperationsRef = useRef(0);
  const activeMeetingIdRef = useRef(meeting.id);

  // Sidebar context
  const { setCurrentMeeting, setMeetings } = useSidebar();

  // Sync aiSummary state when summaryData prop changes (fixes display of fetched summaries)
  useEffect(() => {
    console.log('[useMeetingData] Syncing summary data from prop:', summaryData ? 'present' : 'null');
    setAiSummary(summaryData);
  }, [summaryData]); // Only trigger when parent prop changes, not when aiSummary changes

  useEffect(() => {
    activeMeetingIdRef.current = meeting.id;
    meetingTitleRef.current = initialMeetingTitle;
    savedMeetingTitleRef.current = initialMeetingTitle;
    titleSaveQueueRef.current = new MeetingTitleSaveQueue();
    setMeetingTitle(initialMeetingTitle);
    setIsTitleDirty(false);
    setIsEditingTitle(false);
  }, [initialMeetingTitle, meeting.id]);

  // Handlers
  const handleTitleChange = useCallback((newTitle: string) => {
    meetingTitleRef.current = newTitle;
    setMeetingTitle(newTitle);
    setIsTitleDirty(normalizeMeetingTitle(newTitle) !== savedMeetingTitleRef.current);
  }, []);

  const handleSummaryChange = useCallback((newSummary: Summary) => {
    setAiSummary(newSummary);
  }, []);

  const publishMeetingTitle = useCallback((title: string) => {
    setMeetings((current) => current.map((item) =>
      item.id === meeting.id ? { ...item, title } : item
    ));
    setCurrentMeeting((current) => {
      if (!current || current.id !== meeting.id) return current;
      return { ...current, title };
    });
  }, [meeting.id, setCurrentMeeting, setMeetings]);

  const handleSaveMeetingTitle = useCallback(async () => {
    const meetingId = meeting.id;
    const titleToSave = normalizeMeetingTitle(meetingTitleRef.current);
    const save = titleSaveQueueRef.current.enqueue(titleToSave, async () => {
      try {
        if (!titleToSave) throw new Error(t('Meeting title cannot be empty'));

        if (
          activeMeetingIdRef.current === meetingId
          && titleToSave === savedMeetingTitleRef.current
        ) {
          if (isLatestMeetingTitle(meetingTitleRef.current, titleToSave)) setIsTitleDirty(false);
          return;
        }

        await invokeTauri('api_save_meeting_title', {
          meetingId,
          title: titleToSave,
        });

        // The drawer can navigate while a queued IPC write is in flight. The
        // write still belongs to its original meeting, but it must not mutate
        // the newly opened meeting's local state.
        if (activeMeetingIdRef.current !== meetingId) return;

        savedMeetingTitleRef.current = titleToSave;
        const isLatestEdit = isLatestMeetingTitle(meetingTitleRef.current, titleToSave);
        if (isLatestEdit) {
          meetingTitleRef.current = titleToSave;
          setMeetingTitle(titleToSave);
          setIsTitleDirty(false);

          // Keep every view over the shared archive in sync immediately. Do not
          // publish an intermediate saved title when a newer edit is pending.
          publishMeetingTitle(titleToSave);
          try {
            await onMeetingUpdated?.();
          } catch (refreshError) {
            // The database write and shared-state update already succeeded. A
            // follow-up refresh must not turn a successful rename into an error.
            console.warn('Meeting title saved, but archive refresh failed:', refreshError);
          }
        }
      } catch (error) {
        console.error('Failed to save meeting title:', error);
        const isActiveMeeting = activeMeetingIdRef.current === meetingId;
        if (isActiveMeeting && isLatestMeetingTitle(meetingTitleRef.current, titleToSave)) {
          const savedTitle = savedMeetingTitleRef.current;
          meetingTitleRef.current = savedTitle;
          setMeetingTitle(savedTitle);
          setIsTitleDirty(false);
          publishMeetingTitle(savedTitle);
        }
        const message = error instanceof Error ? error.message : 'Failed to save meeting title: Unknown error';
        if (isActiveMeeting) {
          setError(message);
          toast.error(t('Failed to update meeting title'), { description: message });
        }
        throw new Error(message);
      }
    });

    await save;
  }, [meeting.id, onMeetingUpdated, publishMeetingTitle, t]);

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
    const normalizedTitle = normalizeMeetingTitle(meetingTitleRef.current);
    if (normalizedTitle === savedMeetingTitleRef.current) {
      meetingTitleRef.current = normalizedTitle;
      setMeetingTitle(normalizedTitle);
      setIsTitleDirty(false);
      return;
    }

    beginSaving();
    try {
      await handleSaveMeetingTitle();
    } catch {
      // handleSaveMeetingTitle owns the single user-facing error for a shared
      // in-flight request, including Enter/blur duplicates.
    } finally {
      finishSaving();
    }
  }, [beginSaving, finishSaving, handleSaveMeetingTitle]);

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
    let titleSaveFailed = false;
    let summarySaveError: unknown = null;

    // Title and summary are independent records. A failure in one must not
    // prevent the other dirty record from being persisted.
    if (isTitleDirty) {
      try {
        await handleSaveMeetingTitle();
      } catch (error) {
        titleSaveFailed = true;
        console.error('Failed to save meeting title while saving all changes:', error);
      }
    }

    if (blockNoteSummaryRef.current?.isDirty) {
      try {
        console.log('💾 Saving BlockNote editor changes...');
        await blockNoteSummaryRef.current.saveSummary();
      } catch (error) {
        summarySaveError = error;
        console.error('Failed to save summary while saving all changes:', error);
      }
    }

    try {
      if (summarySaveError) {
        toast.error(t("Failed to save changes"), { description: String(summarySaveError) });
      }
      if (titleSaveFailed || summarySaveError) return;
      toast.success(t("Changes saved successfully"));
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
