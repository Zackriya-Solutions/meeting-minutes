/**
 * One user-facing choice — "what should happen when a call is detected?" — over the
 * three booleans the backend stores.
 *
 * The backend keeps `auto_meeting_detection` (the detector loop), `auto_listening`
 * (record interactively with a live transcript) and `background_auto_recording`
 * (capture silently, register when the call ends) as independent flags, because the
 * detector composes them: auto-listening takes precedence and background capture
 * picks up what it declines. As settings, though, they are not independent — 8
 * combinations describe 4 behaviours, and two of them read identically to a user.
 * So Settings offers the 4 behaviours and this module maps them to flags.
 */

export type AutoCaptureMode =
  /** Do not watch for calls at all. */
  | 'off'
  /** Watch, and offer to start recording. Nothing records without confirmation. */
  | 'ask'
  /** Record supported native clients immediately, with a live transcript. */
  | 'live'
  /** Capture every detected call silently, transcribe it later. */
  | 'silent';

export const AUTO_CAPTURE_MODES: readonly AutoCaptureMode[] = ['off', 'ask', 'live', 'silent'];

/** The subset of notification settings this choice owns. */
export interface AutoCaptureFlags {
  auto_meeting_detection: boolean;
  auto_listening: boolean;
  background_auto_recording: boolean;
}

/** Modes that need an OS microphone-session signal, so they are macOS-only today. */
export function modeNeedsMicrophoneSignal(mode: AutoCaptureMode): boolean {
  return mode === 'live' || mode === 'silent';
}

/**
 * Which mode a stored flag combination represents.
 *
 * Every combination maps to something, including ones Settings can no longer write:
 * with both recording flags on, the detector records native clients live and only
 * falls back to background capture for the calls auto-listening declines, so `live`
 * is the honest label.
 */
export function modeFromFlags(flags: AutoCaptureFlags): AutoCaptureMode {
  if (!flags.auto_meeting_detection) return 'off';
  if (flags.auto_listening) return 'live';
  if (flags.background_auto_recording) return 'silent';
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
    background_auto_recording: mode === 'silent',
  };
}
