/**
 * One user-facing choice — "what should happen when a call is detected?" — over the
 * three legacy booleans the backend stores.
 *
 * The backend keeps `auto_meeting_detection` (the detector loop), `auto_listening`
 * (record interactively with a live transcript) and the removed
 * `background_auto_recording` flag. Settings no longer exposes or writes silent
 * background capture; the flag remains in the API shape only so older settings can
 * be migrated safely.
 */

export type AutoCaptureMode =
  /** Do not watch for calls at all. */
  | 'off'
  /** Watch, and offer to start recording. Nothing records without confirmation. */
  | 'ask'
  /** Record supported native clients immediately, with a live transcript. */
  | 'live';

export const AUTO_CAPTURE_MODES: readonly AutoCaptureMode[] = ['off', 'ask', 'live'];

/** The subset of notification settings this choice owns. */
export interface AutoCaptureFlags {
  auto_meeting_detection: boolean;
  auto_listening: boolean;
  background_auto_recording: boolean;
}

/** Modes that need an OS microphone-session signal, so they are macOS-only today. */
export function modeNeedsMicrophoneSignal(mode: AutoCaptureMode): boolean {
  return mode === 'live';
}

/**
 * Which mode a stored flag combination represents.
 *
 * A legacy background-only combination is shown as `ask`; the backend migration
 * clears that obsolete flag before detection starts.
 */
export function modeFromFlags(flags: AutoCaptureFlags): AutoCaptureMode {
  if (!flags.auto_meeting_detection) return 'off';
  if (flags.auto_listening) return 'live';
  return 'ask';
}

/**
 * The flags a mode should store. Always writes all three so switching modes cannot
 * leave a stale flag behind that the detector would still act on.
 */
export function flagsForMode(mode: AutoCaptureMode): AutoCaptureFlags {
  return {
    auto_meeting_detection: mode !== 'off',
    auto_listening: mode === 'live',
    background_auto_recording: false,
  };
}
