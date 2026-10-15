import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen, UnlistenFn } from '@tauri-apps/api/event';
import { toast } from 'sonner';
import { SpeakerJobComplete, SpeakerJobStatus } from '@/types';
import { formatSpeakerCount } from '@/lib/speakers';
import { formatNamingResult } from '@/lib/speakerNaming';

interface ErrorPayload { meeting_id: string; kind?: SpeakerJobComplete['kind']; error: string; automatic: boolean; cancelled?: boolean }

export function useSpeakerIdentification(
  meetingId: string | null,
  onComplete: (result: SpeakerJobComplete) => void | Promise<void>,
) {
  const [job, setJob] = useState<SpeakerJobStatus | null>(null);
  const [statusKnown, setStatusKnown] = useState(false);
  // An automatic guess holds the auto-summary from its request until its first event: the
  // command can resolve before the queued event arrives, and `job` is still null in between.
  const [autoNamingPending, setAutoNamingPending] = useState(false);
  const onCompleteRef = useRef(onComplete);
  onCompleteRef.current = onComplete;

  // Listen first, then read the status; a status reply is ignored once any event has arrived,
  // so a job that finishes around mount cannot leave a stale 'running' state behind.
  useEffect(() => {
    setJob(null);
    setStatusKnown(false);
    setAutoNamingPending(false);
    if (!meetingId) return;
    let alive = true;
    let eventSeen = false;
    const unlisteners: UnlistenFn[] = [];
    (async () => {
      const fns = await Promise.all([
        listen<SpeakerJobStatus>('diarization-progress', ({ payload }) => {
          if (payload.meeting_id !== meetingId) return;
          eventSeen = true;
          if (payload.kind === 'naming') setAutoNamingPending(false);
          setJob(payload);
        }),
        listen<SpeakerJobComplete>('diarization-complete', async ({ payload }) => {
          if (payload.meeting_id !== meetingId) return;
          eventSeen = true;
          if (payload.kind === 'naming') setAutoNamingPending(false);
          setJob(null);
          await onCompleteRef.current(payload);
          if (payload.automatic) return;
          if (payload.kind === 'naming') {
            toast.success(formatNamingResult(payload.named, payload.suggested));
            return;
          }
          const identified = `Identified ${formatSpeakerCount(payload.speaker_count)}`;
          if (payload.warning) {
            toast.warning(identified, { description: payload.warning });
          } else {
            toast.success(identified);
          }
        }),
        listen<ErrorPayload>('diarization-error', ({ payload }) => {
          if (payload.meeting_id !== meetingId) return;
          eventSeen = true;
          if (payload.kind === 'naming') setAutoNamingPending(false);
          setJob(null);
          if (payload.automatic) return;
          if (payload.cancelled) {
            toast.info(payload.kind === 'naming' ? 'Name guessing cancelled' : 'Speaker identification cancelled');
          } else {
            toast.error(payload.error);
          }
        }),
      ]);
      if (!alive) {
        fns.forEach((u) => u());
        return;
      }
      unlisteners.push(...fns);
      try {
        const status = await invoke<SpeakerJobStatus | null>('get_speaker_identification_status', { meetingId });
        if (alive && !eventSeen) setJob(status);
      } catch (error) {
        console.error('Failed to read speaker identification status:', error);
      } finally {
        if (alive) setStatusKnown(true);
      }
    })();
    return () => {
      alive = false;
      unlisteners.forEach((u) => u());
    };
  }, [meetingId]);

  // Events drive `job`: the backend emits the queued status before this invoke resolves, so
  // writing state here could overwrite a newer event (for example an immediate failure).
  const start = useCallback(async (folderPath: string, numSpeakers: number | null) => {
    if (!meetingId) return;
    await invoke('start_speaker_identification', { meetingId, meetingFolderPath: folderPath, numSpeakers });
  }, [meetingId]);

  const cancel = useCallback(async () => {
    if (!meetingId) return;
    try {
      await invoke('cancel_speaker_identification', { meetingId });
    } catch (error) {
      console.error('Failed to cancel speaker identification:', error);
      // The backend has no such job: resync so a stale banner clears.
      try {
        setJob(await invoke<SpeakerJobStatus | null>('get_speaker_identification_status', { meetingId }));
      } catch (statusError) {
        console.error('Failed to read speaker identification status:', statusError);
      }
    }
  }, [meetingId]);

  // Queues the naming stage; like `start`, the job state comes from the events.
  // An automatic guess on a cloud model runs only with `allowCloud`; the backend declines it
  // otherwise, and no event follows.
  const guessNames = useCallback(async (automatic: boolean, allowCloud?: boolean) => {
    if (!meetingId) return;
    if (automatic) setAutoNamingPending(true);
    try {
      const queued = await invoke<boolean>('api_guess_speaker_names', {
        meetingId,
        automatic,
        allowCloud: allowCloud ?? null,
      });
      if (!queued && automatic) setAutoNamingPending(false);
    } catch (error) {
      // Refused (for example another job runs): no event will follow.
      if (automatic) setAutoNamingPending(false);
      throw error;
    }
  }, [meetingId]);

  return { job, isActive: job !== null, statusKnown, start, cancel, guessNames, autoNamingPending };
}
