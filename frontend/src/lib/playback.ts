/**
 * (clock_s, file_s) points from the backend, ascending in both columns. Transcript rows use the
 * clock; the audio element and the WAV clips use container time. The backend sends the identity,
 * because container time is the recording clock. Two points may share a clock (a gap), so a
 * checkpoint table could be sent instead without changing the player.
 */
export type TimeTable = ReadonlyArray<readonly [number, number]>;

type Axis = 0 | 1;

/** Index of the last of `count` ascending values (`valueAt(i)`) that is at most `x`, or -1. */
function lastAtOrBefore(count: number, valueAt: (i: number) => number, x: number): number {
  let lo = 0;
  let hi = count - 1;
  let found = -1;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    if (valueAt(mid) <= x) {
      found = mid;
      lo = mid + 1;
    } else {
      hi = mid - 1;
    }
  }
  return found;
}

/** Piecewise-linear map from one axis to the other, clamped to the first and last points. */
function interpolate(table: TimeTable, from: Axis, x: number): number {
  if (table.length === 0) return x;
  const to: Axis = from === 0 ? 1 : 0;
  const i = lastAtOrBefore(table.length, (k) => table[k][from], x);
  if (i < 0) return table[0][to];
  const a = table[i];
  if (i === table.length - 1) return a[to];
  const b = table[i + 1];
  const span = b[from] - a[from];
  return span > 0 ? a[to] + ((x - a[from]) * (b[to] - a[to])) / span : a[to];
}

/** Playback position of a transcript time; at a clock shared by two points the later one wins. */
export function clockToFile(table: TimeTable, clockS: number): number {
  return interpolate(table, 0, clockS);
}

/** Transcript time of a playback position; a gap between two points maps to their shared clock. */
export function fileToClock(table: TimeTable, fileS: number): number {
  return interpolate(table, 1, fileS);
}

/** Index of the row playing at `clockS` (rows sorted by start), -1 before the first row. */
export function rowIndexAtTime(rows: ReadonlyArray<{ timestamp: number }>, clockS: number): number {
  return lastAtOrBefore(rows.length, (i) => rows[i].timestamp, clockS);
}

/** Playback has reached the end of the last loaded row and another page exists. */
export function needsMoreRows(rows: ReadonlyArray<{ timestamp: number; endTime?: number }>, clockS: number, hasMore: boolean): boolean {
  if (!hasMore) return false;
  const last = rows[rows.length - 1];
  return !last || clockS >= (last.endTime ?? last.timestamp);
}

/** Length of one WAV clip in the fallback mode. */
export const CLIP_SECONDS = 30;
/** The next clip is requested this long before the current one ends. */
export const CLIP_PREFETCH_SECONDS = 5;

export function shouldPrefetchNextClip(positionInClipS: number, clipLengthS: number): boolean {
  return clipLengthS > 0 && clipLengthS - positionInClipS <= CLIP_PREFETCH_SECONDS;
}

const MEDIA_ERR_SRC_NOT_SUPPORTED = 4;

/** WAV clips when the webview cannot play AAC (for example WebKitGTK without the libav plugin). */
export function choosePlaybackMode(input: { canPlayAac: string; forced: boolean; mediaErrorCode: number | null }): 'asset' | 'clip' {
  if (input.forced || input.canPlayAac === '' || input.mediaErrorCode === MEDIA_ERR_SRC_NOT_SUPPORTED) return 'clip';
  return 'asset';
}

/** Developer switch: set to "1" to test the clip fallback on any system. */
export const FORCE_CLIP_KEY = 'meetily.forceClipPlayback';

export function readForceClipPlayback(): boolean {
  try {
    const value = globalThis.localStorage?.getItem(FORCE_CLIP_KEY);
    return value === '1' || value === 'true';
  } catch {
    return false;
  }
}

export type FollowState = { following: boolean; showBack: boolean };

/**
 * A manual scroll stops following the playing row and offers "Back to playback"; going back,
 * playing from a row or pausing follows again (the next play starts on the current row).
 */
export function followAlong(state: FollowState, event: 'manual-scroll' | 'back' | 'play-from-row' | 'paused'): FollowState {
  const next = event === 'manual-scroll' ? { following: false, showBack: true } : { following: true, showBack: false };
  return next.following === state.following && next.showBack === state.showBack ? state : next;
}

/** Input types that take typed text; the seek bar (`range`), checkboxes and buttons are not among them. */
const TEXT_INPUT_TYPES = new Set(['', 'text', 'search', 'email', 'url', 'tel', 'password', 'number']);

/** Space types into these instead of toggling playback: text inputs, text areas and editable content. */
export function isTypingTarget(
  el: { tagName?: string; type?: string; isContentEditable?: boolean; closest?: (s: string) => unknown } | null,
): boolean {
  if (!el) return false;
  const tag = el.tagName?.toLowerCase();
  if (tag === 'input') return TEXT_INPUT_TYPES.has((el.type ?? '').toLowerCase());
  if (tag === 'textarea' || el.isContentEditable) return true;
  return !!el.closest?.('[contenteditable]:not([contenteditable="false"])');
}
