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
): Promise<void> {
  const invited = normalizeParticipants(participants);
  if (invited.length === 0) return;
  await invoke('set_meeting_participants', {
    meetingId,
    participants: invited,
    source: 'outlook_calendar',
  });
}
