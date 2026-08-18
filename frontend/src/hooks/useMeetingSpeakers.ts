import { useState, useCallback, useEffect, useMemo, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { SpeakerInfo, DiarizationCompletePayload } from "@/types";
import { normalizedSpeakerName } from "@/lib/summarySpeakerLinks";

interface UseMeetingSpeakersProps {
    /** The saved meeting whose speakers we track (null while none is open). */
    meetingId: string | null;
    /**
     * Called after diarization completes (either via the `diarization-complete`
     * event or an explicit detect action) so the caller can refresh transcripts
     * — the segments' `speaker_id`s change once speakers are assigned.
     */
    onDiarized?: () => void | Promise<void>;
}

interface UseMeetingSpeakersReturn {
    /** Raw speaker list for the meeting (name, confirmation state, segment count). */
    speakers: SpeakerInfo[];
    /** id → display_name lookup passed into `resolveSpeakerLabel`. */
    speakersById: Map<number, string>;
    /** Diarized profile ids explicitly confirmed as the local user. */
    selfSpeakerIds: ReadonlySet<number>;
    /** Re-load speakers from the backend. */
    refetchSpeakers: () => Promise<void>;
    /** Persist a new name via `rename_speaker` and update the local map so labels re-render. */
    renameSpeaker: (speakerId: number, displayName: string) => Promise<void>;
    /** Attribute a single unattributed transcript line to a voice the user picked. */
    assignSegmentSpeaker: (transcriptId: string, speakerId: number) => Promise<void>;
    /** Create a confirmed named speaker and attribute one unattributed line to them. */
    addAndAssignSegmentSpeaker: (transcriptId: string, displayName: string) => Promise<void>;
    /** Persist the owner identity on the voice profile, never on an audio channel. */
    setSelfSpeaker: (speakerId: number, isSelf: boolean) => Promise<void>;
}

/**
 * Owns the speaker identities for the currently open saved meeting.
 *
 * Kept separate from `usePaginatedTranscripts` so speaker concerns stay
 * self-contained, but co-locates the `diarization-complete` subscription here
 * (the one place that already knows the active `meetingId`) and delegates the
 * transcript refresh back to the caller via `onDiarized`.
 */
export function useMeetingSpeakers({
    meetingId,
    onDiarized,
}: UseMeetingSpeakersProps): UseMeetingSpeakersReturn {
    const [speakers, setSpeakers] = useState<SpeakerInfo[]>([]);
    const [speakersById, setSpeakersById] = useState<Map<number, string>>(new Map());
    const selfSpeakerIds = useMemo(
        () => new Set(speakers.filter((speaker) => speaker.is_self).map((speaker) => speaker.id)),
        [speakers],
    );

    const refetchSpeakers = useCallback(async () => {
        if (!meetingId) {
            setSpeakers([]);
            setSpeakersById(new Map());
            return;
        }
        try {
            // Existing meetings may have been diarized before automatic name inference
            // was introduced. The command is local and idempotent, so attempting it on
            // open backfills provisional names without requiring another diarization run.
            try {
                await invoke<number>("infer_meeting_speaker_names", { meetingId });
            } catch (err) {
                console.warn("Failed to infer meeting speaker names:", err);
            }
            const result = await invoke<SpeakerInfo[]>("get_meeting_speakers", { meetingId });
            setSpeakers(result);
            setSpeakersById(new Map(result.map((s) => [s.id, s.display_name])));
        } catch (err) {
            // Non-fatal: leave existing labels (channel tags still resolve).
            console.error("Failed to load meeting speakers:", err);
        }
    }, [meetingId]);

    // Load speakers whenever the open meeting changes.
    useEffect(() => {
        refetchSpeakers();
    }, [refetchSpeakers]);

    // Attributing one line is not renaming a voice: the speaker already exists and keeps its
    // name, and only this row's `speaker_id` changes. The transcript is refetched by the
    // caller, which owns the rows.
    const assignSegmentSpeaker = useCallback(async (transcriptId: string, speakerId: number) => {
        if (!meetingId) return;
        await invoke("assign_segment_speaker", { meetingId, transcriptId, speakerId });
    }, [meetingId]);

    const addAndAssignSegmentSpeaker = useCallback(async (transcriptId: string, displayName: string) => {
        if (!meetingId) return;
        await invoke("add_and_assign_segment_speaker", {
            meetingId,
            transcriptId,
            displayName: displayName.trim(),
        });
    }, [meetingId]);

    const renameSpeaker = useCallback(async (speakerId: number, displayName: string) => {
        const trimmed = displayName.trim();
        if (!trimmed) return;

        await invoke("rename_speaker", { speakerId, displayName: trimmed });

        // Optimistically update local state so every label re-renders immediately.
        setSpeakers((prev) =>
            prev.map((speaker) => {
                if (speaker.id !== speakerId) return speaker;
                const aliases = [...(speaker.aliases ?? [])];
                if (
                    speaker.display_name.trim()
                    && normalizedSpeakerName(speaker.display_name) !== normalizedSpeakerName(trimmed)
                    && !aliases.some(
                        (alias) => normalizedSpeakerName(alias) === normalizedSpeakerName(speaker.display_name),
                    )
                ) {
                    aliases.push(speaker.display_name.trim());
                }
                return {
                    ...speaker,
                    display_name: trimmed,
                    aliases,
                    is_confirmed: true,
                };
            })
        );
        setSpeakersById((prev) => {
            const next = new Map(prev);
            next.set(speakerId, trimmed);
            return next;
        });
    }, []);

    const setSelfSpeaker = useCallback(async (speakerId: number, isSelf: boolean) => {
        await invoke("set_self_speaker", { speakerId, isSelf });
        setSpeakers((previous) => previous.map((speaker) => ({
            ...speaker,
            is_self: speaker.id === speakerId ? isSelf : (isSelf ? false : speaker.is_self),
        })));
    }, []);

    // Live refresh: when the backend finishes diarizing this meeting, reload
    // speakers and let the caller refresh transcripts.
    const onDiarizedRef = useRef(onDiarized);
    useEffect(() => {
        onDiarizedRef.current = onDiarized;
    }, [onDiarized]);

    useEffect(() => {
        if (!meetingId) return;

        const unlisteners: (() => void)[] = [];
        let cancelled = false;

        listen<DiarizationCompletePayload>("diarization-complete", async (event) => {
            if (event.payload.meeting_id !== meetingId) return;
            await refetchSpeakers();
            await onDiarizedRef.current?.();
        }).then((un) => {
            if (cancelled) un();
            else unlisteners.push(un);
        });

        // The post-meeting refinement pass replaces transcript rows even when
        // diarization is skipped (models absent), so refresh on its completion too.
        listen<{ meeting_id: string }>("refinement-complete", async (event) => {
            if (event.payload.meeting_id !== meetingId) return;
            await refetchSpeakers();
            await onDiarizedRef.current?.();
        }).then((un) => {
            if (cancelled) un();
            else unlisteners.push(un);
        });

        return () => {
            cancelled = true;
            unlisteners.forEach((un) => un());
        };
    }, [meetingId, refetchSpeakers]);

    return {
        speakers,
        speakersById,
        selfSpeakerIds,
        refetchSpeakers,
        assignSegmentSpeaker,
        addAndAssignSegmentSpeaker,
        renameSpeaker,
        setSelfSpeaker,
    };
}
