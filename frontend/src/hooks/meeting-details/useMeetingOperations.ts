import { useCallback } from 'react';
import { invoke as invokeTauri } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import { useT } from '@/lib/i18n';

interface UseMeetingOperationsProps {
  meeting: any;
}

export function useMeetingOperations({
  meeting,
}: UseMeetingOperationsProps) {
  const t = useT();

  // Open meeting folder in file explorer
  const handleOpenMeetingFolder = useCallback(async () => {
    try {
      await invokeTauri('open_meeting_folder', { meetingId: meeting.id });
    } catch (error) {
      console.error('Failed to open meeting folder:', error);
      toast.error(error as string || t('Failed to open recording folder'));
    }
  }, [meeting.id]);

  return {
    handleOpenMeetingFolder,
  };
}
