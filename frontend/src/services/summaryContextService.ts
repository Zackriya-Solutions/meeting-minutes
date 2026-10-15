import { invoke } from '@tauri-apps/api/core';
import type { MeetingSummaryContext } from '@/lib/summary-context';

export const summaryContextService = {
  getMeeting: (meetingId: string) =>
    invoke<MeetingSummaryContext>('api_get_meeting_summary_context', { meetingId }),
  saveMeeting: (meetingId: string, context: string, useInherited: boolean) =>
    invoke<void>('api_save_meeting_summary_context', { meetingId, context, useInherited }),

  getGlobal: async () =>
    (await invoke<{ context: string }>('api_get_global_summary_context')).context,
  saveGlobal: (context: string) =>
    invoke<void>('api_save_global_summary_context', { context }),
};
