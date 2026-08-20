import { useEffect, useMemo, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { TranscriptSegmentData } from '@/types';
import type { TranscriptReaction } from '@/components/MeetingConversation/ProceduralSpeakerAvatar';

const REACTION_BATCH_SIZE = 24;

type ReactionResult = {
  id: string;
  reaction: TranscriptReaction;
};

export function useTranscriptReactions(
  segments: TranscriptSegmentData[],
  streamingSegmentId: string | null = null,
) {
  const [classified, setClassified] = useState<Record<string, TranscriptReaction>>({});
  const requested = useRef(new Set<string>());

  useEffect(() => {
    const candidates = segments
      .filter((segment) => (
        segment.id !== streamingSegmentId
        && segment.text.trim()
        && !segment.reaction
        && !requested.current.has(segment.id)
      ))
      .slice(-REACTION_BATCH_SIZE);

    if (candidates.length === 0) return;
    candidates.forEach((segment) => requested.current.add(segment.id));

    let cancelled = false;
    invoke<ReactionResult[]>('classify_transcript_reactions', {
      messages: candidates.map(({ id, text }) => ({ id, text })),
    })
      .then((results) => {
        if (cancelled) return;
        setClassified((current) => {
          const next = { ...current };
          results.forEach(({ id, reaction }) => { next[id] = reaction; });
          return next;
        });
      })
      .catch((error) => {
        console.warn('DeepSeek reaction classification is unavailable:', error);
        if (cancelled) return;
        setClassified((current) => {
          const next = { ...current };
          candidates.forEach(({ id }) => { next[id] = 'neutral'; });
          return next;
        });
      });

    return () => { cancelled = true; };
  }, [classified, segments, streamingSegmentId]);

  return useMemo(() => {
    const resolved = new Map<string, TranscriptReaction>();
    segments.forEach((segment) => {
      resolved.set(segment.id, segment.reaction ?? classified[segment.id] ?? 'neutral');
    });
    return resolved;
  }, [classified, segments]);
}
