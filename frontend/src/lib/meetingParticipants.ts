/**
 * The people a calendar entry invited, carried from the start of a recording to the
 * moment the meeting row exists.
 *
 * The recorder learns them at start (from the calendar entry the user tapped, or from
 * the Outlook meeting under way), but there is nothing to attach them to until the
 * stop sequence has saved the meeting — which can be an hour later. Session storage
 * bridges that gap; the slot is one-shot, so a recording that fails to save cannot
 * hand its invitees to the next one.
 */

import { invoke } from '@tauri-apps/api/core';
import { normalizeParticipants } from '@/lib/autoStartRecording';

export const PENDING_PARTICIPANTS_KEY = 'pendingMeetingParticipants';

/**
 * People the user names by hand while the recording runs.
 *
 * Kept apart from the calendar list above because the two are different claims and are
 * stored under different sources: an invitation says who was asked, a roster says who is in
 * the room. Unlike the calendar slot this one is read repeatedly — the recorder shows it
 * back and lets the user edit it — so it is not one-shot until the meeting is saved.
 */
export const PENDING_ROSTER_KEY = 'pendingMeetingRoster';

export const SOURCE_MANUAL_ROSTER = 'manual_roster';

/** The roster as it stands. Empty when nothing was typed, or when storage is unreadable. */
export function readMeetingRoster(storage: Storage): string[] {
  try {
    const parsed: unknown = JSON.parse(storage.getItem(PENDING_ROSTER_KEY) ?? '[]');
    return Array.isArray(parsed) ? normalizeParticipants(parsed as unknown[]) : [];
  } catch {
    return [];
  }
}

/** Replace the roster. An empty list clears the slot rather than storing "[]". */
export function writeMeetingRoster(storage: Storage, names: readonly string[]): void {
  const roster = normalizeParticipants(names);
  if (roster.length > 0) {
    storage.setItem(PENDING_ROSTER_KEY, JSON.stringify(roster));
  } else {
    storage.removeItem(PENDING_ROSTER_KEY);
  }
}

/** The roster, read once and cleared — for the stop sequence that attaches it. */
export function takeMeetingRoster(storage: Storage): string[] {
  const roster = readMeetingRoster(storage);
  storage.removeItem(PENDING_ROSTER_KEY);
  return roster;
}

/** Remember the invited people for the recording that is starting now. */
export function rememberMeetingParticipants(
  storage: Storage,
  participants: readonly string[] | null | undefined,
): void {
  const invited = normalizeParticipants(participants);
  if (invited.length > 0) {
    storage.setItem(PENDING_PARTICIPANTS_KEY, JSON.stringify(invited));
  } else {
    storage.removeItem(PENDING_PARTICIPANTS_KEY);
  }
}

/** The remembered people, read once and cleared. Empty when there are none. */
export function takeMeetingParticipants(storage: Storage): string[] {
  const stored = storage.getItem(PENDING_PARTICIPANTS_KEY);
  storage.removeItem(PENDING_PARTICIPANTS_KEY);
  if (!stored) return [];
  try {
    const parsed: unknown = JSON.parse(stored);
    return Array.isArray(parsed) ? normalizeParticipants(parsed as unknown[]) : [];
  } catch {
    return [];
  }
}

/** Attach the invited people to a saved meeting. No-op for an empty list. */
export async function saveMeetingParticipants(
  meetingId: string,
  participants: readonly string[],
  source: string = 'outlook_calendar',
): Promise<void> {
  const invited = normalizeParticipants(participants);
  if (invited.length === 0) return;
  await invoke('set_meeting_participants', {
    meetingId,
    participants: invited,
    source,
  });
}
