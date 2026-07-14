/**
 * Marked moments — timestamps (elapsed seconds from recording start) the user
 * flags with the "Момент" button / Cmd+M during a recording.
 *
 * They are persisted in localStorage, keyed by meeting id. During recording the
 * only id available is the temporary IndexedDB id (`meeting-<ts>`); once the
 * meeting is saved to the backend it gets a stable id, so `migrateMarkedMoments`
 * moves the marks onto the final id at save time. meeting-details then reads
 * them back with `getMarkedMoments(meeting.id)`.
 */

const keyFor = (meetingId: string) => `memento:marks:${meetingId}`;

function read(meetingId: string): number[] {
  if (typeof window === 'undefined' || !meetingId) return [];
  try {
    const raw = window.localStorage.getItem(keyFor(meetingId));
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    return Array.isArray(parsed)
      ? parsed.filter((n): n is number => typeof n === 'number' && Number.isFinite(n))
      : [];
  } catch {
    return [];
  }
}

function write(meetingId: string, marks: number[]): void {
  if (typeof window === 'undefined' || !meetingId) return;
  try {
    // Normalize to whole seconds, dedupe, and keep chronological order.
    const normalized = Array.from(new Set(marks.map((n) => Math.max(0, Math.floor(n))))).sort(
      (a, b) => a - b,
    );
    if (normalized.length === 0) {
      window.localStorage.removeItem(keyFor(meetingId));
    } else {
      window.localStorage.setItem(keyFor(meetingId), JSON.stringify(normalized));
    }
  } catch (error) {
    console.warn('Failed to persist marked moments:', error);
  }
}

/** Marks (whole seconds, ascending) recorded for a meeting. */
export function getMarkedMoments(meetingId: string | null | undefined): number[] {
  return meetingId ? read(meetingId) : [];
}

/** Add a mark at `seconds` elapsed and return the updated list. */
export function addMarkedMoment(meetingId: string | null | undefined, seconds: number): number[] {
  if (!meetingId) return [];
  write(meetingId, [...read(meetingId), seconds]);
  return read(meetingId);
}

/**
 * Move marks from a temporary recording id onto the final saved meeting id.
 * Merges with anything already stored under the target id and drops the source.
 */
export function migrateMarkedMoments(
  fromMeetingId: string | null | undefined,
  toMeetingId: string | null | undefined,
): void {
  if (!fromMeetingId || !toMeetingId || fromMeetingId === toMeetingId) return;
  const from = read(fromMeetingId);
  if (from.length === 0) return;
  write(toMeetingId, [...read(toMeetingId), ...from]);
  try {
    window.localStorage.removeItem(keyFor(fromMeetingId));
  } catch {
    /* ignore */
  }
}
