'use client';

import { Button } from '@/components/ui/button';
import { Progress } from '@/components/ui/progress';
import { SpeakerJobStatus } from '@/types';

/** What a job waiting in the queue is waiting for. */
const QUEUED: Record<SpeakerJobStatus['kind'], string> = {
  identify: 'Waiting to identify speakers…',
  naming: 'Waiting to find names…',
};

export function SpeakerJobBanner({ job, onCancel }: { job: SpeakerJobStatus | null; onCancel: () => void }) {
  if (!job) return null;
  const queued = job.state === 'queued';
  return (
    <div className="flex items-center justify-between gap-3 border-b border-blue-100 bg-blue-50 px-4 py-2 text-sm text-blue-900">
      <div className="flex min-w-0 flex-1 items-center gap-2">
        <div className="h-3 w-3 shrink-0 animate-spin rounded-full border-2 border-blue-300 border-t-blue-700" />
        {/* The message may carry its own detail (for example the download percentage). */}
        <span className="truncate">{queued ? QUEUED[job.kind] : job.message}</span>
        {!queued && <Progress value={job.percent} className="h-1.5 w-24 shrink-0" />}
        <span className="hidden truncate text-xs text-blue-700 md:inline">Speaker editing resumes when this finishes</span>
      </div>
      <Button size="sm" variant="ghost" onClick={onCancel}>Cancel</Button>
    </div>
  );
}
