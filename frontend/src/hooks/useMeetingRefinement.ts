import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { toast } from 'sonner';
import { useT } from '@/lib/i18n';

/** Stages emitted by the Rust refinement pass (`audio/refinement.rs`). */
export type RefinementStage =
    | 'waiting_for_model'
    | 'diarizing'
    | 'decoding'
    | 'transcribing'
    | 'attributing'
    | 'retranscribing'
    | 'exporting';

interface RefinementProgressPayload {
    meeting_id: string;
    stage: RefinementStage;
    done: number;
    total: number;
}

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
            }),
        );

        track(
            listen<{ meeting_id: string }>('refinement-complete', async (event) => {
                if (event.payload.meeting_id !== meetingId) return;
                setRunning(false);
                setStage(null);
                setProgress(null);
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
                );
            }),
        );

        return () => {
            cancelled = true;
            unlisteners.forEach((un) => un());
        };
    }, [meetingId, t]);

    const rerun = useCallback(async () => {
        if (!meetingId) return;
        try {
            await invoke('rerun_meeting_refinement', { meetingId });
            setRunning(true);
            toast.info(t('Reprocessing started. This takes a few minutes.'));
        } catch (error) {
            toast.error(typeof error === 'string' ? error : t('Reprocessing failed'));
        }
    }, [meetingId, t]);

    const stageLabels: Record<RefinementStage, string> = {
        waiting_for_model: t('Waiting for the speech model'),
        diarizing: t('Separating voices'),
        decoding: t('Reading the recording'),
        transcribing: t('Splitting replies'),
        attributing: t('Labelling speakers'),
        retranscribing: t('Re-transcribing'),
        exporting: t('Saving'),
    };

    let label: string | null = null;
    if (running) {
        label = stage ? stageLabels[stage] : t('Processing');
        if (stage === 'transcribing' && progress) {
            label = `${label} ${progress.done}/${progress.total}`;
        }
    }

    return { running, stage, label, rerun };
}
