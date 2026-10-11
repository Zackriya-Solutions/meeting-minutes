import { afterAll, describe, expect, test } from 'bun:test';
import {
  CLIP_PREFETCH_SECONDS, CLIP_SECONDS, FORCE_CLIP_KEY, choosePlaybackMode,
  clockToFile, fileToClock, followAlong, isTypingTarget, needsMoreRows, readForceClipPlayback, rowIndexAtTime,
  shouldPrefetchNextClip, type FollowState, type TimeTable,
} from '../../src/lib/playback';

afterAll(() => { Reflect.deleteProperty(globalThis, 'localStorage'); });

const near = (actual: number, expected: number) => expect(Math.abs(actual - expected)).toBeLessThan(1e-6);
/** What the backend sends for a 62 min live recording: container time is the recording clock. */
const identity: TimeTable = [[0, 0], [3732.02, 3732.02]];

describe('transcript clock and file time', () => {
  test('the identity table maps times to themselves, clamped to the recording', () => {
    near(clockToFile(identity, 3300.2), 3300.2);
    near(fileToClock(identity, 61.25), 61.25);
    near(clockToFile(identity, -1), 0);
    near(fileToClock(identity, 4000), 3732.02);
  });

  test('a table with a gap: a boundary clock starts the next segment and the gap maps to the boundary', () => {
    // The shape a checkpoint table would have, should the backend send one.
    const live: TimeTable = [[0, 0.02], [30, 30.02], [30, 30.06], [60, 60.06]];
    near(clockToFile(live, 29.5), 29.52);
    near(clockToFile(live, 30), 30.06);
    near(fileToClock(live, 30.04), 30);
    near(fileToClock(live, 0.01), 0);
    near(fileToClock(live, 45.06), 45);
  });

  test('an empty table leaves times unchanged', () => {
    near(clockToFile([], 12.5), 12.5);
    near(fileToClock([], 12.5), 12.5);
  });
});

describe('rows at a time', () => {
  const rows = [{ timestamp: 2 }, { timestamp: 5 }, { timestamp: 9 }];

  test('rowIndexAtTime before the first row, on a start, between rows and after the last', () => {
    expect(rowIndexAtTime(rows, 1)).toBe(-1);
    expect(rowIndexAtTime(rows, 5)).toBe(1);
    expect(rowIndexAtTime(rows, 7.5)).toBe(1);
    expect(rowIndexAtTime(rows, 100)).toBe(2);
    expect(rowIndexAtTime([], 3)).toBe(-1);
  });

  test('the row at 55 min of a live recording is the row at the playback position', () => {
    const halfSecondRows = Array.from({ length: 40 }, (_, i) => ({ timestamp: 3290 + i * 0.5 }));
    const position = clockToFile(identity, 3300.2); // <audio>.currentTime while that row plays
    expect(rowIndexAtTime(halfSecondRows, fileToClock(identity, position))).toBe(20);
  });

  test('needsMoreRows once playback reaches the end of the last loaded row', () => {
    const loaded = [{ timestamp: 0, endTime: 4 }, { timestamp: 4, endTime: 9 }];
    expect(needsMoreRows(loaded, 8.9, true)).toBe(false);
    expect(needsMoreRows(loaded, 9, true)).toBe(true);
    expect(needsMoreRows(loaded, 20, false)).toBe(false);
    expect(needsMoreRows([{ timestamp: 4 }], 4, true)).toBe(true);
    expect(needsMoreRows([], 0, true)).toBe(true);
  });
});

describe('clip fallback', () => {
  test('the next clip is fetched in the last seconds of the current one', () => {
    expect(CLIP_SECONDS).toBe(30);
    expect(shouldPrefetchNextClip(24.9, 30)).toBe(false);
    expect(shouldPrefetchNextClip(30 - CLIP_PREFETCH_SECONDS, 30)).toBe(true);
    expect(shouldPrefetchNextClip(29.9, 30)).toBe(true);
  });

  test('choosePlaybackMode falls back to clips only when the file cannot be played', () => {
    expect(choosePlaybackMode({ canPlayAac: 'maybe', forced: false, mediaErrorCode: null })).toBe('asset');
    expect(choosePlaybackMode({ canPlayAac: 'probably', forced: false, mediaErrorCode: 3 })).toBe('asset');
    expect(choosePlaybackMode({ canPlayAac: '', forced: false, mediaErrorCode: null })).toBe('clip');
    expect(choosePlaybackMode({ canPlayAac: 'maybe', forced: true, mediaErrorCode: null })).toBe('clip');
    expect(choosePlaybackMode({ canPlayAac: 'maybe', forced: false, mediaErrorCode: 4 })).toBe('clip');
  });

  test('the developer switch is read from localStorage and tolerates a missing or failing store', () => {
    expect(readForceClipPlayback()).toBe(false);
    const values = new Map<string, string>();
    Object.defineProperty(globalThis, 'localStorage', {
      configurable: true, value: { getItem: (key: string) => values.get(key) ?? null },
    });
    expect(readForceClipPlayback()).toBe(false);
    values.set(FORCE_CLIP_KEY, '1');
    expect(readForceClipPlayback()).toBe(true);
    Object.defineProperty(globalThis, 'localStorage', {
      configurable: true, value: { getItem: () => { throw new Error('blocked'); } },
    });
    expect(readForceClipPlayback()).toBe(false);
  });
});

describe('follow along and the Space key', () => {
  test('followAlong pauses on a manual scroll and resumes on back or play from a row', () => {
    const following: FollowState = { following: true, showBack: false };
    const scrolled = followAlong(following, 'manual-scroll');
    expect(scrolled).toEqual({ following: false, showBack: true });
    expect(followAlong(scrolled, 'back')).toEqual(following);
    expect(followAlong(scrolled, 'play-from-row')).toEqual(following);
    expect(followAlong(scrolled, 'paused')).toEqual(following);
    expect(followAlong(following, 'paused')).toBe(following); // unchanged state keeps its identity
  });

  test('isTypingTarget for text inputs, text areas and editable content, not for the seek bar or buttons', () => {
    expect(isTypingTarget({ tagName: 'INPUT' })).toBe(true);
    expect(isTypingTarget({ tagName: 'INPUT', type: 'text' })).toBe(true);
    expect(isTypingTarget({ tagName: 'INPUT', type: 'search' })).toBe(true);
    expect(isTypingTarget({ tagName: 'TEXTAREA' })).toBe(true);
    expect(isTypingTarget({ tagName: 'DIV', isContentEditable: true })).toBe(true);
    expect(isTypingTarget({ tagName: 'P', isContentEditable: false, closest: (s) => (s.includes('contenteditable') ? {} : null) })).toBe(true);
    expect(isTypingTarget({ tagName: 'INPUT', type: 'range', closest: () => null })).toBe(false); // the seek bar
    expect(isTypingTarget({ tagName: 'INPUT', type: 'checkbox', closest: () => null })).toBe(false);
    expect(isTypingTarget({ tagName: 'SELECT', closest: () => null })).toBe(false);
    expect(isTypingTarget({ tagName: 'BUTTON', isContentEditable: false, closest: () => null })).toBe(false);
    expect(isTypingTarget(null)).toBe(false);
  });
});
