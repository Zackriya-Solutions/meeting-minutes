'use client';

import { useEffect } from 'react';
import { usePathname, useRouter } from 'next/navigation';
import { useRecordingState } from '@/contexts/RecordingStateContext';
import { isRecordingNavigationLocked } from '@/lib/recordingNavigation';

export function RecordingNavigationGuard() {
  const pathname = usePathname();
  const router = useRouter();
  const { isRecording, status } = useRecordingState();
  const locked = isRecordingNavigationLocked(isRecording, status);

  useEffect(() => {
    if (locked && pathname !== '/recording') {
      router.replace('/recording', { scroll: false });
    }
  }, [locked, pathname, router]);

  return null;
}
