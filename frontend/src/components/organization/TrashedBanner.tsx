'use client';

import { useState } from 'react';
import { toast } from 'sonner';
import { organizationService } from '@/services/organizationService';

export function TrashedBanner({ meetingId, deletedAt }: { meetingId: string; deletedAt?: string | null }) {
  // Hides the banner right after a restore, before the meeting metadata is reloaded.
  const [restoredId, setRestoredId] = useState<string | null>(null);

  if (!deletedAt || restoredId === meetingId) return null;

  const restore = async () => {
    try {
      await organizationService.restoreMeeting(meetingId);
      setRestoredId(meetingId);
      toast.success('Meeting restored');
    } catch (error) {
      toast.error(String(error));
    }
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
