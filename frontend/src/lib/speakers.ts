import { invoke } from '@tauri-apps/api/core';
import { MeetingSpeaker, SuggestionSource } from '@/types';

const PALETTE = [
  { chip: 'bg-blue-100 text-blue-800 border-blue-200', dot: 'bg-blue-500' },
  { chip: 'bg-emerald-100 text-emerald-800 border-emerald-200', dot: 'bg-emerald-500' },
  { chip: 'bg-amber-100 text-amber-800 border-amber-200', dot: 'bg-amber-500' },
  { chip: 'bg-purple-100 text-purple-800 border-purple-200', dot: 'bg-purple-500' },
  { chip: 'bg-rose-100 text-rose-800 border-rose-200', dot: 'bg-rose-500' },
  { chip: 'bg-cyan-100 text-cyan-800 border-cyan-200', dot: 'bg-cyan-500' },
  { chip: 'bg-lime-100 text-lime-800 border-lime-200', dot: 'bg-lime-500' },
  { chip: 'bg-orange-100 text-orange-800 border-orange-200', dot: 'bg-orange-500' },
];

function keyIndex(key: string): number | null {
  const match = /^spk_(\d+)$/.exec(key);
  return match ? Number(match[1]) : null;
}

export function defaultSpeakerLabel(key: string): string {
  const index = keyIndex(key);
  return index === null ? key : `Speaker ${index + 1}`;
}

export function buildSpeakerNameMap(speakers: MeetingSpeaker[]): Record<string, string> {
  return Object.fromEntries(
    speakers.map((s) => [s.speaker_key, s.display_name?.trim() || defaultSpeakerLabel(s.speaker_key)]),
  );
}

export function speakerLabel(key: string, names: Record<string, string>): string {
  return names[key] ?? defaultSpeakerLabel(key);
}

export function speakerColor(key: string): { chip: string; dot: string } {
  const index = keyIndex(key) ?? 0;
  return PALETTE[index % PALETTE.length];
}

export async function fetchSpeakerNames(meetingId: string): Promise<Record<string, string>> {
  try {
    return buildSpeakerNameMap(await invoke<MeetingSpeaker[]>('api_list_meeting_speakers', { meetingId }));
  } catch (error) {
    console.error('Failed to load meeting speakers:', error);
    return {};
  }
}

/** True when row `index` starts a run of rows by the same speaker. */
export function isSpeakerRunStart(segments: ReadonlyArray<{ speaker?: string | null }>, index: number): boolean {
  const current = segments[index]?.speaker;
  return !!current && current !== segments[index - 1]?.speaker;
}

/**
 * The speaker control a transcript row shows: the chip where a speaker's run starts, a compact
 * control on the run's other rows while editing is possible, otherwise none.
 */
export function rowSpeakerControl(
  speakerKey: string | null | undefined,
  isRunStart: boolean,
  editable: boolean,
): { speakerKey: string; compact: boolean } | null {
  if (!speakerKey || (!isRunStart && !editable)) return null;
  return { speakerKey, compact: !isRunStart };
}

export function formatSpeakerCount(count: number): string {
  return `${count} speaker${count === 1 ? '' : 's'}`;
}

export function formatPropagationMessage(count: number): string {
  return `Also named in ${count} other meeting${count === 1 ? '' : 's'}`;
}

export type SpeakerNameState =
  | { kind: 'default' }
  /** Typed or confirmed by the user, or set before people existed */
  | { kind: 'named'; name: string }
  | { kind: 'auto'; name: string; source: 'voice' | 'conversation' }
  | { kind: 'suggestion'; name: string; reason: string | null; source: SuggestionSource };

/** What a chip shows besides its label: an automatic name to confirm, or a suggestion. */
export function speakerNameState(speaker: MeetingSpeaker | undefined): SpeakerNameState {
  if (!speaker) return { kind: 'default' };
  const name = speaker.display_name?.trim();
  if (name) {
    if (speaker.name_source === 'voice' || speaker.name_source === 'conversation') {
      return { kind: 'auto', name, source: speaker.name_source };
    }
    return { kind: 'named', name };
  }
  const suggested = speaker.suggested_name?.trim();
  if (suggested && speaker.suggestion_source) {
    return { kind: 'suggestion', name: suggested, reason: speaker.suggestion_reason, source: speaker.suggestion_source };
  }
  return { kind: 'default' };
}
