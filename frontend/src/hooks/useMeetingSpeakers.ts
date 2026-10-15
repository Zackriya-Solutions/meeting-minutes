import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import { MeetingSpeaker, NameOutcome, PropagatedLink } from '@/types';
import { buildSpeakerNameMap, formatPropagationMessage } from '@/lib/speakers';

async function undoPropagation(links: PropagatedLink[]) {
  try {
    await invoke<string[]>('api_undo_name_propagation', { links });
  } catch (error) {
    console.error('Failed to undo name propagation:', error);
    toast.error('Failed to undo');
  }
}

/** `onNamed` reloads what a typed or confirmed name changes besides the speakers (the people list). */
export function useMeetingSpeakers(meetingId: string | null, onNamed?: () => Promise<void>) {
  const [speakers, setSpeakers] = useState<MeetingSpeaker[]>([]);
  const onNamedRef = useRef(onNamed);
  onNamedRef.current = onNamed;

  /** Reloads and returns the speakers, so a caller can decide on the fresh list. */
  const refetch = useCallback(async (): Promise<MeetingSpeaker[]> => {
    if (!meetingId) {
      setSpeakers([]);
      return [];
    }
    try {
      const next = await invoke<MeetingSpeaker[]>('api_list_meeting_speakers', { meetingId });
      setSpeakers(next);
      return next;
    } catch (error) {
      console.error('Failed to load meeting speakers:', error);
      return [];
    }
  }, [meetingId]);

  useEffect(() => {
    void refetch();
  }, [refetch]);

  const names = useMemo(() => buildSpeakerNameMap(speakers), [speakers]);

  // Naming a voice can name the same voice in other meetings; offer to take that back.
  // The backend links at most one speaker in each other meeting.
  const applyName = useCallback(async (command: string, args: { speakerKey: string; name?: string }) => {
    const outcome = await invoke<NameOutcome>(command, { meetingId, ...args });
    await Promise.all([refetch(), onNamedRef.current?.()]);
    const links = outcome.propagated;
    if (links.length === 0) return;
    toast.success(formatPropagationMessage(links.length), {
      action: { label: 'Undo', onClick: () => { void undoPropagation(links); } },
      duration: 10000,
    });
  }, [meetingId, refetch]);

  const name = useCallback(
    (speakerKey: string, name: string) => applyName('api_name_meeting_speaker', { speakerKey, name }),
    [applyName],
  );

  const confirm = useCallback(
    (speakerKey: string) => applyName('api_confirm_meeting_speaker_name', { speakerKey }),
    [applyName],
  );

  const reject = useCallback(async (speakerKey: string) => {
    await invoke('api_reject_meeting_speaker_name', { meetingId, speakerKey });
    await refetch();
  }, [meetingId, refetch]);

  const merge = useCallback(async (fromKey: string, intoKey: string) => {
    await invoke('api_merge_meeting_speakers', { meetingId, fromKey, intoKey });
    await refetch();
  }, [meetingId, refetch]);

  const reassign = useCallback(async (transcriptId: string, speakerKey: string | null) => {
    const key = await invoke<string>('api_set_transcript_speaker', { meetingId, transcriptId, speakerKey });
    await refetch();
    return key;
  }, [meetingId, refetch]);

  return { speakers, names, refetch, name, confirm, reject, merge, reassign };
}
