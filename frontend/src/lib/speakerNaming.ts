import type { MeetingSpeaker } from '@/types';

/** A speaker who still talks in the transcript and has no name. */
export function hasUnnamedSpeaker(speakers: MeetingSpeaker[]): boolean {
  return speakers.some((s) => s.row_count > 0 && !s.display_name?.trim());
}

/**
 * Whether to guess names right after Identify. The transcript goes to the summary model; the
 * backend runs an automatic guess only on a local model unless `allowCloud` is set, which it is
 * when the user already sends transcripts to the summary model by turning on auto-summary.
 * Returns the request, or null for no guess.
 */
export function decideAutoGuessNames(input: {
  speakerIdentification: boolean;
  isAutoSummary: boolean;
  speakers: MeetingSpeaker[];
}): { allowCloud: boolean } | null {
  if (!input.speakerIdentification || !hasUnnamedSpeaker(input.speakers)) return null;
  return { allowCloud: input.isAutoSummary };
}

export function formatNamingResult(named: number, suggested: number): string {
  if (named > 0 && suggested > 0) return `Named ${named}, suggested ${suggested}`;
  if (named > 0) return `Named ${named}`;
  if (suggested > 0) return `Suggested ${suggested}`;
  return 'No names found';
}

/** The auto-summary waits (up to its cap) for Identify and for the automatic name guess it starts. */
export function isWaitingForSpeakers(s: { expired: boolean; statusKnown: boolean; isActive: boolean; autoNamingPending: boolean }): boolean {
  return !s.expired && (!s.statusKnown || s.isActive || s.autoNamingPending);
}
