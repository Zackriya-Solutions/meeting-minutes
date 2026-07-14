/**
 * In-recording diarization preferences — the control pill's "Speaker ID" toggle
 * and expected-speaker-count stepper.
 *
 * During recording the meeting only has its temporary IndexedDB id
 * (`meeting-<ts>`), so choices are held in localStorage keyed by that id (same
 * pattern as marked moments). Once the meeting is saved and gets its stable id,
 * `useRecordingStop` reads them back with `takeDiarizationPrefs` and persists
 * them onto the meeting row via the `set_meeting_diarization_prefs` command.
 */

export interface RecordingDiarizationPrefs {
  /** Detect speakers for this meeting after it ends. */
  speakerIdEnabled: boolean;
  /** Expected number of speakers; null = automatic estimation. */
  expectedSpeakers: number | null;
}

export const DEFAULT_DIARIZATION_PREFS: RecordingDiarizationPrefs = {
  speakerIdEnabled: true,
  expectedSpeakers: null,
};

/**
 * Stepper bounds — must stay within the backend's 1..=32 validation range.
 * The UI floor is 2: below it the stepper returns to Auto (a 1-speaker hint is
 * never more useful than automatic estimation).
 */
export const MIN_EXPECTED_SPEAKERS = 2;
export const MAX_EXPECTED_SPEAKERS = 12;

const keyFor = (meetingId: string) => `memento:diarization:${meetingId}`;

/**
 * Prefs stored for a recording, or the defaults when the user never touched
 * the pill (or storage is unavailable).
 */
export function getDiarizationPrefs(
  meetingId: string | null | undefined,
): RecordingDiarizationPrefs {
  if (typeof window === 'undefined' || !meetingId) return { ...DEFAULT_DIARIZATION_PREFS };
  try {
    const raw = window.localStorage.getItem(keyFor(meetingId));
    if (!raw) return { ...DEFAULT_DIARIZATION_PREFS };
    const parsed = JSON.parse(raw);
    const enabled =
      typeof parsed?.speakerIdEnabled === 'boolean'
        ? parsed.speakerIdEnabled
        : DEFAULT_DIARIZATION_PREFS.speakerIdEnabled;
    const count =
      typeof parsed?.expectedSpeakers === 'number' &&
      Number.isInteger(parsed.expectedSpeakers) &&
      parsed.expectedSpeakers >= MIN_EXPECTED_SPEAKERS &&
      parsed.expectedSpeakers <= MAX_EXPECTED_SPEAKERS
        ? parsed.expectedSpeakers
        : null;
    return { speakerIdEnabled: enabled, expectedSpeakers: count };
  } catch {
    return { ...DEFAULT_DIARIZATION_PREFS };
  }
}

/** Persist the pill's current choices for the (temporary) recording id. */
export function setDiarizationPrefs(
  meetingId: string | null | undefined,
  prefs: RecordingDiarizationPrefs,
): void {
  if (typeof window === 'undefined' || !meetingId) return;
  try {
    window.localStorage.setItem(keyFor(meetingId), JSON.stringify(prefs));
  } catch (error) {
    console.warn('Failed to persist diarization prefs:', error);
  }
}

/**
 * Read-and-remove the prefs recorded under the temporary id at save time.
 * Returns null when the user never touched the pill — the caller then skips
 * the backend call and the meeting keeps the default behavior.
 */
export function takeDiarizationPrefs(
  meetingId: string | null | undefined,
): RecordingDiarizationPrefs | null {
  if (typeof window === 'undefined' || !meetingId) return null;
  const raw = (() => {
    try {
      return window.localStorage.getItem(keyFor(meetingId));
    } catch {
      return null;
    }
  })();
  if (!raw) return null;
  const prefs = getDiarizationPrefs(meetingId);
  try {
    window.localStorage.removeItem(keyFor(meetingId));
  } catch {
    /* ignore */
  }
  return prefs;
}
