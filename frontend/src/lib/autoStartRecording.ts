/**
 * Hand-off for a recording the user asked for from somewhere other than the recorder:
 * the home screen's "new meeting", a calendar entry, the tray, the detection banner.
 *
 * The request has to survive a route change, so it is left in session storage and
 * consumed by `useRecordingStart` once the recorder mounts. Keeping both keys and both
 * halves of the protocol here is what stops a title from outliving the request that
 * set it — a stale one would silently rename the next unrelated recording.
 */

export const AUTO_START_FLAG_KEY = 'autoStartRecording';
export const AUTO_START_TITLE_KEY = 'autoStartRecordingTitle';

/**
 * Ask the recorder to start as soon as it mounts, optionally under a name the caller
 * already knows (a calendar subject). Always writes both keys: an untitled request
 * must clear a title left behind by an earlier one.
 */
export function requestAutoStart(storage: Storage, title?: string | null): void {
  storage.setItem(AUTO_START_FLAG_KEY, 'true');
  const requested = title?.trim();
  if (requested) {
    storage.setItem(AUTO_START_TITLE_KEY, requested);
  } else {
    storage.removeItem(AUTO_START_TITLE_KEY);
  }
}

/** Withdraw a pending request without starting anything. */
export function clearAutoStartRequest(storage: Storage): void {
  storage.removeItem(AUTO_START_FLAG_KEY);
  storage.removeItem(AUTO_START_TITLE_KEY);
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
