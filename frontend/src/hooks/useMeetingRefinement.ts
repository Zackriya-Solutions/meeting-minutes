import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { toast } from 'sonner';
import { useT } from '@/lib/i18n';
import {
    refinementLabel,
    type RefinementProgressPayload,
    type RefinementStage,
} from '@/lib/refinementProgress';

export type { RefinementStage } from '@/lib/refinementProgress';

export interface MeetingRefinement {
    /** True between `refinement-started` and `-complete`/`-error`. */
    running: boolean;
    /** Current stage, or null before the first progress event arrives. */
    stage: RefinementStage | null;
    /** Localized one-line description of the current stage, or null when idle. */
    label: string | null;
    /** Ask for another pass. Resolves once it is spawned, not when it finishes. */
    rerun: () => Promise<void>;
}

/**
 * Tracks the post-meeting refinement pass for one meeting, and triggers it on demand.
 *
 * The pass is minutes of heavy CPU — the diarization cascade, then one ASR call per
 * speaker turn — and used to run with no user-visible sign at all: the events existed
 * but nothing listened, so the app looked idle while the fans spun. This subscribes to
 * them so a caller can say which stage is running.
 *
 * @param meetingId Meeting to follow; pass undefined to disable.
 * @param onFinished Called after a completed pass so the caller can refetch rows.
 */
export function useMeetingRefinement(
    meetingId?: string,
    onFinished?: () => Promise<void> | void,
): MeetingRefinement {
    const t = useT();
    const toastId = meetingId ? `meeting-refinement:${meetingId}` : undefined;
    const [running, setRunning] = useState(false);
    const [stage, setStage] = useState<RefinementStage | null>(null);
    const [progress, setProgress] = useState<{ done: number; total: number } | null>(null);

    // Kept in a ref so a caller passing an inline closure does not resubscribe every render.
    const onFinishedRef = useRef(onFinished);
    useEffect(() => {
        onFinishedRef.current = onFinished;
    }, [onFinished]);

    useEffect(() => {
        if (!meetingId) return;

        const unlisteners: (() => void)[] = [];
        let cancelled = false;
        const track = (promise: Promise<() => void>) => {
            promise.then((un) => {
                if (cancelled) un();
                else unlisteners.push(un);
            });
        };

        track(
            listen<{ meeting_id: string }>('refinement-started', (event) => {
                if (event.payload.meeting_id !== meetingId) return;
                setRunning(true);
                setStage(null);
                setProgress(null);
                toast.loading(t('Processing'), {
                    id: toastId,
                    duration: Infinity,
                });
            }),
        );

        track(
            listen<RefinementProgressPayload>('refinement-progress', (event) => {
                if (event.payload.meeting_id !== meetingId) return;
                // A pass can start before this component mounts (it fires on save), so a
                // progress event is itself proof that one is running.
                setRunning(true);
                setStage(event.payload.stage);
                setProgress(
                    event.payload.total > 0
                        ? { done: event.payload.done, total: event.payload.total }
                        : null,
                );
                toast.loading(
                    refinementLabel(
                        true,
                        event.payload.stage,
                        event.payload.total > 0
                            ? { done: event.payload.done, total: event.payload.total }
                            : null,
                        t,
                    ),
                    {
                        id: toastId,
                        duration: Infinity,
                    },
                );
            }),
        );

        track(
            listen<{ meeting_id: string }>('refinement-complete', async (event) => {
                if (event.payload.meeting_id !== meetingId) return;
                setRunning(false);
                setStage(null);
                setProgress(null);
                toast.success(t('Meeting processing complete'), { id: toastId });
                await onFinishedRef.current?.();
            }),
        );

        track(
            listen<{ meeting_id: string; error?: string }>('refinement-error', (event) => {
                if (event.payload.meeting_id !== meetingId) return;
                setRunning(false);
                setStage(null);
                setProgress(null);
                // Silence was the original complaint: a pass that dies must say so.
                toast.error(
                    event.payload.error
                        ? `${t('Reprocessing failed')}: ${event.payload.error}`
                        : t('Reprocessing failed'),
                    { id: toastId },
                );
            }),
        );

        return () => {
            cancelled = true;
            unlisteners.forEach((un) => un());
            if (toastId) toast.dismiss(toastId);
        };
    }, [meetingId, t, toastId]);

    const rerun = useCallback(async () => {
        if (!meetingId) return;
        try {
            await invoke('rerun_meeting_refinement', { meetingId });
            setRunning(true);
            toast.loading(t('Processing'), {
                id: toastId,
                duration: Infinity,
            });
        } catch (error) {
            toast.error(
                typeof error === 'string' ? error : t('Reprocessing failed'),
                { id: toastId },
            );
        }
    }, [meetingId, t, toastId]);

    const label = refinementLabel(running, stage, progress, t);

    return { running, stage, label, rerun };
}
