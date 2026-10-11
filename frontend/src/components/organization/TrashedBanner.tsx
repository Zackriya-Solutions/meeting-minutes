'use client';

import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import { useSidebar } from '@/components/Sidebar/SidebarProvider';
import { organizationService } from '@/services/organizationService';

export function TrashedBanner({ meetingId }: { meetingId: string }) {
  const { refetchMeetings, refetchOrganization } = useSidebar();
  const [trashed, setTrashed] = useState(false);

  useEffect(() => {
    let alive = true;
    invoke<{ deleted_at: string | null }>('api_get_meeting_metadata', { meetingId })
      .then(meta => { if (alive) setTrashed(Boolean(meta.deleted_at)); })
      .catch(() => { if (alive) setTrashed(false); });
    return () => { alive = false; };
  }, [meetingId]);

  if (!trashed) return null;

  const restore = async () => {
    try {
      await organizationService.restoreMeeting(meetingId);
      setTrashed(false);
      toast.success('Meeting restored');
    } catch (error) {
      toast.error(String(error));
    }
    await Promise.all([refetchMeetings(), refetchOrganization()]);
  };

  return (
    <div className="flex items-center justify-between border-b border-amber-200 bg-amber-50 px-4 py-2 text-sm text-amber-900">
      <span>This meeting is in Trash.</span>
      <button type="button" onClick={() => void restore()} className="rounded-md px-3 py-1 font-medium hover:bg-amber-100">
        Restore
      </button>
    </div>
  );
}
