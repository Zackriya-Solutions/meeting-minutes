/**
 * Hand-off for a recording the user asked for from somewhere other than the recorder:
 * the home screen's "new meeting", a calendar entry, the tray, the detection banner.
 *
 * The request has to survive a route change, so it is left in session storage and
 * consumed by `useRecordingStart` once the recorder mounts. Keeping every key and both
 * halves of the protocol here is what stops a title from outliving the request that
 * set it — a stale one would silently rename the next unrelated recording, or invite
 * the wrong people to it.
 */

export const AUTO_START_FLAG_KEY = 'autoStartRecording';
export const AUTO_START_TITLE_KEY = 'autoStartRecordingTitle';
export const AUTO_START_PARTICIPANTS_KEY = 'autoStartRecordingParticipants';

/**
 * Ask the recorder to start as soon as it mounts, optionally under a name the caller
 * already knows (a calendar subject) and with the people that entry invited. Always
 * writes every key: an untitled request must clear what an earlier one left behind.
 */
export function requestAutoStart(
  storage: Storage,
  title?: string | null,
  participants?: readonly string[] | null,
): void {
  storage.setItem(AUTO_START_FLAG_KEY, 'true');
  const requested = title?.trim();
  if (requested) {
    storage.setItem(AUTO_START_TITLE_KEY, requested);
  } else {
    storage.removeItem(AUTO_START_TITLE_KEY);
  }
  const invited = normalizeParticipants(participants);
  if (invited.length > 0) {
    storage.setItem(AUTO_START_PARTICIPANTS_KEY, JSON.stringify(invited));
  } else {
    storage.removeItem(AUTO_START_PARTICIPANTS_KEY);
  }
}

/** Withdraw a pending request without starting anything. */
export function clearAutoStartRequest(storage: Storage): void {
  storage.removeItem(AUTO_START_FLAG_KEY);
  storage.removeItem(AUTO_START_TITLE_KEY);
  storage.removeItem(AUTO_START_PARTICIPANTS_KEY);
}

/**
 * The requested name, read once and cleared, so a later unattended start falls back to
 * a generated timestamp instead of reusing this meeting's name. Null when the request
 * carried no usable title.
 */
export function takeRequestedMeetingTitle(storage: Storage): string | null {
  const requested = storage.getItem(AUTO_START_TITLE_KEY);
  storage.removeItem(AUTO_START_TITLE_KEY);
  return requested?.trim() || null;
}

/**
 * The invited people, read once and cleared for the same reason as the title. Empty
 * when the request carried none, or when the stored value is unreadable.
 */
export function takeRequestedMeetingParticipants(storage: Storage): string[] {
  const stored = storage.getItem(AUTO_START_PARTICIPANTS_KEY);
  storage.removeItem(AUTO_START_PARTICIPANTS_KEY);
  if (!stored) return [];
  try {
    const parsed: unknown = JSON.parse(stored);
    return Array.isArray(parsed) ? normalizeParticipants(parsed as unknown[]) : [];
  } catch {
    return [];
  }
}

/** Trim, drop blanks, and deduplicate case-insensitively, keeping the first spelling. */
export function normalizeParticipants(
  participants?: readonly unknown[] | null,
): string[] {
  if (!participants) return [];
  const seen = new Set<string>();
  const invited: string[] = [];
  for (const participant of participants) {
    if (typeof participant !== 'string') continue;
    const name = participant.trim();
    if (!name) continue;
    const key = name.toLocaleLowerCase();
    if (seen.has(key)) continue;
    seen.add(key);
    invited.push(name);
  }
  return invited;
}
