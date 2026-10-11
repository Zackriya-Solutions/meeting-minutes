import { afterAll, afterEach, beforeEach, describe, expect, mock, test } from 'bun:test';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';
import type { PlaybackSource } from '../../src/types';
import type { PlaybackClock, PlaybackControls } from '../../src/hooks/usePlayback';
import { FORCE_CLIP_KEY } from '../../src/lib/playback';
import { FakeAudio, audios } from '../fixtures/audio';

Object.defineProperty(globalThis, 'Audio', { configurable: true, value: FakeAudio });

const originalCore = { ...await import('@tauri-apps/api/core') };
afterAll(() => {
  mock.module('@tauri-apps/api/core', () => originalCore);
  Reflect.deleteProperty(globalThis, 'Audio');
});
/** What the backend sends: container time is the recording clock, so the table is the identity. */
const IDENTITY_SOURCE: PlaybackSource = {
  path: '/recordings/meeting/audio.mp4',
  duration_s: 60,
  time_table: [[0, 0], [60, 60]],
};
let source: PlaybackSource = IDENTITY_SOURCE;
const fullClip = () => new ArrayBuffer(44 + 30 * 16_000 * 2);
/** While set, clip renders wait for it, which keeps a clip "loading" for the test. */
let renderGate: Promise<void> | null = null;
let renderResult: () => ArrayBuffer | number[] = fullClip;
const invoke = mock(async (command: string, _args?: Record<string, unknown>): Promise<unknown> => {
  if (command === 'api_prepare_meeting_playback') return source;
  if (command === 'api_render_playback_clip') {
    if (renderGate) await renderGate;
    return renderResult();
  }
  throw new Error(`Unexpected command: ${command}`);
});
const convertFileSrc = (path: string) => `asset://localhost/${encodeURIComponent(path)}`;
mock.module('@tauri-apps/api/core', () => ({ ...originalCore, invoke, convertFileSrc }));
const { usePlayback, usePlaybackClock, usePlaybackClockSelector, CLIP_SEEK_DELAY_MS } = await import('../../src/hooks/usePlayback');

let controls: PlaybackControls;
/** Renders of the component that owns the hook, and of the clock subscribers below it. */
const renders = { player: 0, clock: 0, tens: 0 };
let shownClock = -1;
let shownTens = -1;
function ClockView({ clock }: { clock: PlaybackClock }) {
  renders.clock += 1;
  shownClock = usePlaybackClock(clock);
  return null;
}
function TensView({ clock }: { clock: PlaybackClock }) {
  renders.tens += 1;
  shownTens = usePlaybackClockSelector(clock, (clockS) => Math.floor(clockS / 10));
  return null;
}
function Player() {
  renders.player += 1;
  controls = usePlayback('meeting-a', true);
  return <><ClockView clock={controls.clock} /><TensView clock={controls.clock} /></>;
}
let renderer: ReactTestRenderer | undefined;
async function settle() {
  await act(async () => { await new Promise((resolve) => setTimeout(resolve, 0)); });
}
async function mount() {
  await act(async () => { renderer = create(<Player />); });
  await settle();
  return audios[audios.length - 1];
}
const clipStarts = () => invoke.mock.calls
  .filter(([command]) => command === 'api_render_playback_clip')
  .map(([, args]) => (args as { startFileS: number }).startFileS);
const near = (actual: number, expected: number) => expect(Math.abs(actual - expected)).toBeLessThan(1e-9);

beforeEach(() => {
  audios.length = 0;
  FakeAudio.canPlay = 'maybe';
  source = IDENTITY_SOURCE;
  invoke.mockClear();
  renderGate = null;
  renderResult = fullClip;
});
afterEach(async () => {
  if (renderer) await act(async () => renderer!.unmount());
  renderer = undefined;
  Reflect.deleteProperty(globalThis, 'localStorage');
  Reflect.deleteProperty(globalThis, 'document');
});

describe('usePlayback', () => {
  test('prepares the source and plays from a clock time', async () => {
    const audio = await mount();
    expect(invoke).toHaveBeenCalledWith('api_prepare_meeting_playback', { meetingId: 'meeting-a' });
    expect(controls.ready).toBe(true);
    expect(controls.durationS).toBe(60);
    expect(audio.src).toBe('asset://localhost/%2Frecordings%2Fmeeting%2Faudio.mp4');
    await act(async () => { controls.playFrom(45); });
    near(audio.currentTime, 45);
    expect(audio.paused).toBe(false);
    expect(controls.playing).toBe(true);
    audio.currentTime = 50;
    await act(async () => { audio.emit('timeupdate'); });
    near(controls.clock.get(), 50);
  });

  test('positions go through the time table the backend sends', async () => {
    source = { ...IDENTITY_SOURCE, time_table: [[0, 0.02], [30, 30.02], [30, 30.06], [60, 60.06]] };
    const audio = await mount();
    await act(async () => { controls.playFrom(45); });
    near(audio.currentTime, 45.06);
    audio.currentTime = 50.06;
    await act(async () => { audio.emit('timeupdate'); });
    near(controls.clock.get(), 50);
  });

  test('refresh loads the time table again without restarting playback', async () => {
    FakeAudio.canPlay = '';
    const audio = await mount();
    await act(async () => { controls.playFrom(45); });
    await settle();
    near(clipStarts()[0], 45);
    // A retranscription has replaced the rows: they now count the decoded file's frames.
    source = { ...IDENTITY_SOURCE, time_table: [[0, 0], [0.02, 0], [30.02, 30], [30.06, 30], [60.06, 60]] };
    await act(async () => { await controls.refresh(); });
    expect(invoke.mock.calls.filter(([command]) => command === 'api_prepare_meeting_playback')).toHaveLength(2);
    expect(audios).toHaveLength(1);
    expect(audio.paused).toBe(false);
    expect(clipStarts()).toHaveLength(1);
    audio.currentTime = 5;
    await act(async () => { audio.emit('timeupdate'); });
    near(controls.clock.get(), 50.06);
    await act(async () => { controls.playFrom(45.06); });
    await settle();
    near(clipStarts()[1], 45);
  });

  test('switches to clip mode on src not supported', async () => {
    const audio = await mount();
    await act(async () => { controls.playFrom(45); });
    audio.error = { code: 4 };
    await act(async () => { audio.emit('error'); });
    await settle();
    expect(clipStarts()).toHaveLength(1);
    near(clipStarts()[0], 45);
    expect(invoke).toHaveBeenCalledWith('api_render_playback_clip', expect.objectContaining({ meetingId: 'meeting-a', seconds: 30 }));
    expect(audio.src.startsWith('blob:')).toBe(true);
    expect(audio.paused).toBe(false);
  });

  test('a clip delivered as a number array still plays as audio', async () => {
    // Tauri's postMessage fallback delivers raw command bytes as a JSON array of numbers.
    const wav = new Uint8Array(fullClip());
    wav.set([82, 73, 70, 70]); // RIFF
    renderResult = () => Array.from(wav);
    const blobs: Blob[] = [];
    const createObjectURL = URL.createObjectURL;
    URL.createObjectURL = (blob: Blob) => { blobs.push(blob); return createObjectURL(blob); };
    try {
      const audio = await mount();
      await act(async () => { controls.playFrom(45); });
      audio.error = { code: 4 };
      await act(async () => { audio.emit('error'); });
      await settle();
      expect(blobs).toHaveLength(1);
      expect(blobs[0].size).toBe(wav.length);
      expect(new Uint8Array(await blobs[0].arrayBuffer()).slice(0, 4)).toEqual(new Uint8Array([82, 73, 70, 70]));
      expect(audio.paused).toBe(false);
    } finally {
      URL.createObjectURL = createObjectURL;
    }
  });

  test('forced clip mode from local storage', async () => {
    Object.defineProperty(globalThis, 'localStorage', {
      configurable: true, value: { getItem: (key: string) => (key === FORCE_CLIP_KEY ? '1' : null) },
    });
    const audio = await mount();
    expect(controls.ready).toBe(true);
    expect(audio.src).toBe('');
  });

  test('clip mode prefetches the next clip', async () => {
    FakeAudio.canPlay = '';
    const audio = await mount();
    expect(audio.src).toBe('');
    await act(async () => { controls.playFrom(0); });
    await settle();
    near(clipStarts()[0], 0);
    const first = audio.src;
    expect(first.startsWith('blob:')).toBe(true);
    audio.currentTime = 26;
    await act(async () => { audio.emit('timeupdate'); });
    await settle();
    expect(clipStarts()).toHaveLength(2);
    near(clipStarts()[1], 30);
    audio.ended = true;
    await act(async () => { audio.emit('ended'); });
    await settle();
    expect(audio.src).not.toBe(first);
    expect(clipStarts()).toHaveLength(2); // the prefetched clip was used
    expect(controls.playing).toBe(true);
  });

  test('dragging the seek bar in clip mode renders one clip where the drag stops', async () => {
    FakeAudio.canPlay = '';
    const audio = await mount();
    await act(async () => { controls.playFrom(0); });
    await settle();
    const before = clipStarts().length;
    await act(async () => {
      controls.seek(40);
      controls.seek(45);
      controls.seek(50);
    });
    near(controls.clock.get(), 50);
    expect(clipStarts()).toHaveLength(before); // nothing rendered while the drag goes on
    await act(async () => { await new Promise((resolve) => setTimeout(resolve, CLIP_SEEK_DELAY_MS + 50)); });
    await settle();
    expect(clipStarts()).toHaveLength(before + 1);
    near(clipStarts()[before], 50);
    expect(audio.paused).toBe(false);
  });

  test('a seek while the first clip is still rendering ends up playing at the new position', async () => {
    FakeAudio.canPlay = '';
    const audio = await mount();
    let release!: () => void;
    renderGate = new Promise<void>((resolve) => { release = resolve; });
    await act(async () => { controls.playFrom(0); });
    await act(async () => { controls.seek(40); });
    renderGate = null;
    release();
    await act(async () => { await new Promise((resolve) => setTimeout(resolve, CLIP_SEEK_DELAY_MS + 50)); });
    await settle();
    expect(clipStarts().at(-1)).toBeCloseTo(40, 6);
    expect(audio.paused).toBe(false);
    expect(controls.playing).toBe(true);
  });

  test('toggle while a clip renders pauses without rendering again', async () => {
    FakeAudio.canPlay = '';
    const audio = await mount();
    let release!: () => void;
    renderGate = new Promise<void>((resolve) => { release = resolve; });
    await act(async () => { controls.playFrom(0); });
    expect(clipStarts()).toHaveLength(1);
    await act(async () => { controls.toggle(); });
    expect(clipStarts()).toHaveLength(1);
    expect(controls.playing).toBe(false);
    release();
    await settle();
    expect(audio.paused).toBe(true);
    expect(controls.playing).toBe(false);
  });

  test('playing is false when the clip handoff finds nothing to play after a seek', async () => {
    FakeAudio.canPlay = '';
    const audio = await mount();
    await act(async () => { controls.playFrom(0); });
    await settle();
    let release!: () => void;
    renderGate = new Promise<void>((resolve) => { release = resolve; });
    audio.ended = true;
    audio.paused = true;
    await act(async () => { audio.emit('ended'); });
    await act(async () => { controls.seek(70); });
    renderResult = () => new ArrayBuffer(44); // past the end of the file
    release();
    await act(async () => { await new Promise((resolve) => setTimeout(resolve, CLIP_SEEK_DELAY_MS + 50)); });
    await settle();
    expect(controls.playing).toBe(false);
  });

  test('the first toggle after a clip render error starts playback', async () => {
    FakeAudio.canPlay = '';
    const audio = await mount();
    renderResult = () => { throw new Error('render failed'); };
    await act(async () => { controls.playFrom(0); });
    await settle();
    expect(controls.error).not.toBeNull();
    expect(controls.playing).toBe(false);
    renderResult = fullClip;
    await act(async () => { controls.toggle(); });
    await settle();
    expect(audio.paused).toBe(false);
    expect(controls.playing).toBe(true);
  });

  test('a seek after a media error does not start playback', async () => {
    FakeAudio.canPlay = '';
    const audio = await mount();
    await act(async () => { controls.playFrom(0); });
    await settle();
    expect(clipStarts()).toHaveLength(1);
    audio.error = { code: 3 };
    await act(async () => { audio.emit('error'); });
    expect(controls.error).not.toBeNull();
    await act(async () => { controls.seek(20); });
    await act(async () => { await new Promise((resolve) => setTimeout(resolve, CLIP_SEEK_DELAY_MS + 50)); });
    expect(clipStarts()).toHaveLength(1);
  });

  test('play from with stop pauses at the end', async () => {
    const audio = await mount();
    await act(async () => { controls.playFrom(10, 18); });
    near(audio.currentTime, 10);
    audio.currentTime = 17.5;
    await act(async () => { audio.emit('timeupdate'); });
    expect(audio.paused).toBe(false);
    audio.currentTime = 18.2;
    await act(async () => { audio.emit('timeupdate'); });
    expect(audio.paused).toBe(true);
    expect(controls.playing).toBe(false);
  });

  test('the clock moves without re-rendering the player; only its subscribers follow', async () => {
    const audio = await mount();
    await act(async () => { controls.playFrom(10); });
    const before = { ...renders };
    for (const time of [10.25, 10.5, 10.75]) {
      audio.currentTime = time;
      await act(async () => { audio.emit('timeupdate'); });
    }
    expect(renders.player).toBe(before.player);
    expect(renders.clock).toBe(before.clock + 3);
    near(shownClock, 10.75);
    // A selector re-renders only when what it selects changes.
    expect(renders.tens).toBe(before.tens);
    audio.currentTime = 21;
    await act(async () => { audio.emit('timeupdate'); });
    expect(renders.tens).toBe(before.tens + 1);
    expect(shownTens).toBe(2);
    expect(renders.player).toBe(before.player);
  });

  test('speed changes playback rate', async () => {
    const audio = await mount();
    await act(async () => { controls.setRate(1.5); });
    expect(audio.playbackRate).toBe(1.5);
    expect(controls.rate).toBe(1.5);
  });

  test('Space toggles playback, also on the seek bar and on buttons, but not in a text field', async () => {
    const keyListeners = new Set<(event: unknown) => void>();
    Object.defineProperty(globalThis, 'document', {
      configurable: true,
      value: {
        addEventListener: (type: string, listener: (event: unknown) => void) => { if (type === 'keydown') keyListeners.add(listener); },
        removeEventListener: (_type: string, listener: (event: unknown) => void) => { keyListeners.delete(listener); },
      },
    });
    const press = async (target: object) => {
      let prevented = false;
      const event = { code: 'Space', key: ' ', repeat: false, defaultPrevented: false, target, preventDefault: () => { prevented = true; } };
      await act(async () => { keyListeners.forEach((listener) => listener(event)); });
      return prevented;
    };
    const audio = await mount();
    expect(await press({ tagName: 'INPUT', type: 'range', closest: () => null })).toBe(true);
    expect(audio.paused).toBe(false);
    expect(await press({ tagName: 'BUTTON', closest: () => null })).toBe(true);
    expect(audio.paused).toBe(true);
    expect(await press({ tagName: 'INPUT', type: 'text', closest: () => null })).toBe(false);
    expect(audio.paused).toBe(true);
  });
});
