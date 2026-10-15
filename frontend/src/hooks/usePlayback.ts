'use client';

import { useCallback, useEffect, useRef, useState, useSyncExternalStore } from 'react';
import { convertFileSrc, invoke } from '@tauri-apps/api/core';
import type { PlaybackSource } from '@/types';
import { errorMessage } from '@/lib/errors';
import {
  CLIP_SECONDS, choosePlaybackMode, clockToFile, fileToClock, isTypingTarget, readForceClipPlayback,
  shouldPrefetchNextClip,
} from '@/lib/playback';

export type PlaybackRate = 1 | 1.5 | 2;
export type PlaybackMode = 'asset' | 'clip';

/**
 * Position on the transcript clock, in seconds. It moves on every timeupdate, so it is read
 * through a subscription and only the components that show it re-render.
 */
export interface PlaybackClock {
  get: () => number;
  subscribe: (listener: () => void) => () => void;
}

export interface PlaybackControls {
  ready: boolean;
  error: string | null;
  playing: boolean;
  clock: PlaybackClock;
  durationS: number;
  rate: PlaybackRate;
  /** Plays from `clockS`; with `stopAtClockS`, pauses there (speaker samples). */
  playFrom: (clockS: number, stopAtClockS?: number) => void;
  toggle: () => void;
  seek: (clockS: number) => void;
  setRate: (rate: PlaybackRate) => void;
  /**
   * Loads the time table again. A retranscription times its rows by the decoded file instead of
   * the recording clock, so the table loaded with the meeting no longer places them in the audio.
   */
  refresh: () => Promise<void>;
}

/** In clip mode a seek renders its clip only after the seek bar has been still this long. */
export const CLIP_SEEK_DELAY_MS = 150;

const AAC = 'audio/mp4; codecs="mp4a.40.2"';
const WAV_HEADER_BYTES = 44;
const WAV_BYTES_PER_SECOND = 16_000 * 2;
/** A clip shorter than this was cut by the end of the file. */
const FULL_CLIP_SECONDS = CLIP_SECONDS - 0.01;

interface Clip {
  startFileS: number;
  lengthS: number;
  url: string;
}

function revoke(clip: Clip | null) {
  if (clip) URL.revokeObjectURL(clip.url);
}

function createClock(): PlaybackClock & { set: (clockS: number) => void } {
  let value = 0;
  const listeners = new Set<() => void>();
  return {
    get: () => value,
    set: (clockS) => {
      value = clockS;
      listeners.forEach((listener) => listener());
    },
    subscribe: (listener) => {
      listeners.add(listener);
      return () => { listeners.delete(listener); };
    },
  };
}

/** Re-renders the caller when `select` of the clock changes; return a primitive from it. */
export function usePlaybackClockSelector<T>(clock: PlaybackClock, select: (clockS: number) => T): T {
  const snapshot = () => select(clock.get());
  return useSyncExternalStore(clock.subscribe, snapshot, snapshot);
}

const identity = (clockS: number) => clockS;

/** The clock itself: re-renders the caller on every timeupdate. */
export function usePlaybackClock(clock: PlaybackClock): number {
  return usePlaybackClockSelector(clock, identity);
}

/**
 * Plays a meeting's recording on the transcript clock. The file plays directly over the asset
 * protocol; when the webview cannot decode it, 30 s WAV clips rendered by the backend take over.
 * Both are positioned in container time, which the backend's time table maps to the clock.
 */
export function usePlayback(meetingId: string | null, enabled: boolean): PlaybackControls {
  const [source, setSource] = useState<PlaybackSource | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [playing, setPlaying] = useState(false);
  const [rate, setRateState] = useState<PlaybackRate>(1);
  const [clock] = useState(createClock);

  const meetingIdRef = useRef(meetingId);
  meetingIdRef.current = meetingId;
  const audioRef = useRef<HTMLAudioElement | null>(null);
  const sourceRef = useRef<PlaybackSource | null>(null);
  const modeRef = useRef<PlaybackMode>('asset');
  const clipRef = useRef<Clip | null>(null);
  const nextClipRef = useRef<Promise<Clip | null> | null>(null);
  /** Only the newest clip request may change what plays. */
  const loadIdRef = useRef(0);
  /** A clip-mode seek waiting for the seek bar to stop moving. */
  const seekTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const stopAtRef = useRef<number | null>(null);
  /** The user wants sound: set by play, cleared by pause and by the end. */
  const wantPlayRef = useRef(false);
  /** A clip render started by a play or seek is in flight. */
  const loadingRef = useRef(false);
  const rateRef = useRef<PlaybackRate>(1);

  const cancelPendingSeek = useCallback(() => {
    if (seekTimerRef.current !== null) clearTimeout(seekTimerRef.current);
    seekTimerRef.current = null;
  }, []);

  const report = useCallback((clockS: number) => {
    clock.set(clockS);
    const stopAt = stopAtRef.current;
    if (stopAt !== null && clockS >= stopAt) {
      stopAtRef.current = null;
      wantPlayRef.current = false;
      audioRef.current?.pause();
    }
  }, [clock]);

  const renderClip = useCallback(async (startFileS: number): Promise<Clip | null> => {
    const id = meetingIdRef.current;
    if (!id) return null;
    const raw = await invoke<ArrayBuffer | number[]>('api_render_playback_clip', { meetingId: id, startFileS, seconds: CLIP_SECONDS });
    // Raw bytes arrive as an ArrayBuffer over the IPC protocol, as an array of numbers when
    // Tauri has fallen back to postMessage.
    const bytes = raw instanceof ArrayBuffer ? raw : new Uint8Array(raw).buffer;
    const lengthS = Math.max(0, bytes.byteLength - WAV_HEADER_BYTES) / WAV_BYTES_PER_SECOND;
    if (lengthS <= 0) return null; // past the end of the file
    return { startFileS, lengthS, url: URL.createObjectURL(new Blob([bytes], { type: 'audio/wav' })) };
  }, []);

  const dropNextClip = useCallback(() => {
    const pending = nextClipRef.current;
    nextClipRef.current = null;
    void pending?.then(revoke);
  }, []);

  const showClip = useCallback((clip: Clip, play: boolean) => {
    const audio = audioRef.current;
    if (!audio) {
      revoke(clip);
      return;
    }
    revoke(clipRef.current);
    clipRef.current = clip;
    audio.src = clip.url;
    audio.defaultPlaybackRate = rateRef.current;
    audio.playbackRate = rateRef.current;
    audio.currentTime = 0;
    if (play) void audio.play().catch(() => {});
  }, []);

  const loadClipAt = useCallback(async (fileS: number, play: boolean) => {
    const id = ++loadIdRef.current;
    loadingRef.current = true;
    dropNextClip();
    try {
      const clip = await renderClip(Math.max(0, fileS));
      if (id === loadIdRef.current) loadingRef.current = false;
      if (id !== loadIdRef.current) {
        revoke(clip);
        return;
      }
      if (!clip) {
        wantPlayRef.current = false;
        audioRef.current?.pause();
        setPlaying(false);
        return;
      }
      showClip(clip, play);
    } catch (e) {
      if (id !== loadIdRef.current) return;
      loadingRef.current = false;
      wantPlayRef.current = false;
      setError(errorMessage(e, 'Failed to play the recording'));
      setPlaying(false);
    }
  }, [dropNextClip, renderClip, showClip]);

  const playFrom = useCallback((clockS: number, stopAtClockS?: number) => {
    const audio = audioRef.current;
    const src = sourceRef.current;
    if (!audio || !src) return;
    cancelPendingSeek();
    const target = Math.min(Math.max(0, clockS), src.duration_s);
    setError(null);
    stopAtRef.current = stopAtClockS ?? null;
    wantPlayRef.current = true;
    clock.set(target);
    const fileS = clockToFile(src.time_table, target);
    if (modeRef.current === 'asset') {
      audio.currentTime = fileS;
      void audio.play().catch(() => {});
    } else {
      void loadClipAt(fileS, true);
    }
  }, [cancelPendingSeek, clock, loadClipAt]);

  const toggle = useCallback(() => {
    const audio = audioRef.current;
    if (!audio || !sourceRef.current) return;
    // The user's intent, not audio.paused: the audio is paused while a clip renders.
    if (wantPlayRef.current) {
      wantPlayRef.current = false;
      if (modeRef.current === 'clip' && (loadingRef.current || seekTimerRef.current !== null)) {
        // Cancel the pending render; the clip on hand is not at the clock, so resume renders anew.
        cancelPendingSeek();
        loadIdRef.current += 1;
        loadingRef.current = false;
        dropNextClip();
        revoke(clipRef.current);
        clipRef.current = null;
      }
      audio.pause();
      setPlaying(false);
      return;
    }
    stopAtRef.current = null;
    if (modeRef.current === 'asset' || clipRef.current) {
      wantPlayRef.current = true;
      void audio.play().catch(() => {});
    } else {
      playFrom(clock.get());
    }
  }, [cancelPendingSeek, clock, dropNextClip, playFrom]);

  const seek = useCallback((clockS: number) => {
    const audio = audioRef.current;
    const src = sourceRef.current;
    if (!audio || !src) return;
    const target = Math.min(Math.max(0, clockS), src.duration_s);
    stopAtRef.current = null;
    clock.set(target);
    const fileS = clockToFile(src.time_table, target);
    cancelPendingSeek();
    if (modeRef.current === 'asset') {
      audio.currentTime = fileS;
    } else if (wantPlayRef.current) {
      // Dragging the seek bar seeks on every step; one clip is rendered where the drag stops.
      seekTimerRef.current = setTimeout(() => {
        seekTimerRef.current = null;
        void loadClipAt(fileS, true);
      }, CLIP_SEEK_DELAY_MS);
    } else {
      // Paused: the clip at the new position is rendered when playback resumes.
      loadIdRef.current += 1;
      loadingRef.current = false;
      dropNextClip();
      revoke(clipRef.current);
      clipRef.current = null;
    }
  }, [cancelPendingSeek, clock, dropNextClip, loadClipAt]);

  const setRate = useCallback((next: PlaybackRate) => {
    rateRef.current = next;
    setRateState(next);
    const audio = audioRef.current;
    if (audio) {
      audio.defaultPlaybackRate = next;
      audio.playbackRate = next;
    }
  }, []);

  const refresh = useCallback(async () => {
    const id = meetingIdRef.current;
    if (!id || !sourceRef.current) return;
    try {
      const prepared = await invoke<PlaybackSource>('api_prepare_meeting_playback', { meetingId: id });
      // The audio keeps playing: the file is the same, only the clock of its positions changes.
      if (meetingIdRef.current !== id || !sourceRef.current) return;
      sourceRef.current = prepared;
      setSource(prepared);
    } catch (e) {
      console.warn('Meeting playback could not be refreshed:', e);
    }
  }, []);

  useEffect(() => {
    sourceRef.current = null;
    setSource(null);
    setError(null);
    setPlaying(false);
    clock.set(0);
    if (!enabled || !meetingId || typeof Audio === 'undefined') return;

    let alive = true;
    const audio = new Audio();
    audio.preload = 'metadata';
    audio.defaultPlaybackRate = rateRef.current;
    audio.playbackRate = rateRef.current;
    audioRef.current = audio;
    modeRef.current = choosePlaybackMode({ canPlayAac: audio.canPlayType(AAC), forced: readForceClipPlayback(), mediaErrorCode: null });

    const table = () => sourceRef.current?.time_table ?? [];
    const continuesWithNextClip = () => modeRef.current === 'clip' && (clipRef.current?.lengthS ?? 0) >= FULL_CLIP_SECONDS;

    const onTimeUpdate = () => {
      if (modeRef.current === 'asset') {
        report(fileToClock(table(), audio.currentTime));
        return;
      }
      // A seek is about to replace this clip; its position would pull the seek bar back.
      if (seekTimerRef.current !== null) return;
      const clip = clipRef.current;
      if (!clip) return;
      report(fileToClock(table(), clip.startFileS + audio.currentTime));
      if (!nextClipRef.current && continuesWithNextClip() && shouldPrefetchNextClip(audio.currentTime, clip.lengthS)) {
        nextClipRef.current = renderClip(clip.startFileS + clip.lengthS).catch(() => null);
      }
    };
    const onPlay = () => setPlaying(true);
    const onPause = () => {
      // A clip that ends hands over to the next one; playback has not stopped.
      if (audio.ended && wantPlayRef.current && continuesWithNextClip()) return;
      setPlaying(false);
    };
    const onEnded = async () => {
      const clip = clipRef.current;
      if (!clip || !wantPlayRef.current || !continuesWithNextClip()) {
        wantPlayRef.current = false;
        setPlaying(false);
        return;
      }
      const pending = nextClipRef.current ?? renderClip(clip.startFileS + clip.lengthS).catch(() => null);
      nextClipRef.current = null;
      const id = ++loadIdRef.current;
      loadingRef.current = true;
      const next = await pending;
      if (!alive || id !== loadIdRef.current) {
        revoke(next);
        return;
      }
      loadingRef.current = false;
      if (next && wantPlayRef.current) {
        showClip(next, true);
      } else {
        revoke(next);
        wantPlayRef.current = false;
        setPlaying(false);
      }
    };
    const onError = () => {
      const fallback = modeRef.current === 'asset' && choosePlaybackMode({
        canPlayAac: audio.canPlayType(AAC), forced: false, mediaErrorCode: audio.error?.code ?? null,
      }) === 'clip';
      if (fallback) {
        modeRef.current = 'clip';
        const src = sourceRef.current;
        if (wantPlayRef.current && src) void loadClipAt(clockToFile(src.time_table, clock.get()), true);
        return;
      }
      wantPlayRef.current = false;
      setError('The recording could not be played');
      setPlaying(false);
    };
    const onEndedEvent = () => { void onEnded(); };
    const listeners: Array<[string, () => void]> = [
      ['timeupdate', onTimeUpdate], ['play', onPlay], ['pause', onPause], ['ended', onEndedEvent], ['error', onError],
    ];
    listeners.forEach(([type, listener]) => audio.addEventListener(type, listener));

    invoke<PlaybackSource>('api_prepare_meeting_playback', { meetingId })
      .then((prepared) => {
        if (!alive) return;
        sourceRef.current = prepared;
        setSource(prepared);
        if (modeRef.current === 'asset') audio.src = convertFileSrc(prepared.path);
      })
      .catch((e) => {
        console.warn('Meeting playback is not available:', e);
        if (alive) setError(errorMessage(e, 'The recording is not available'));
      });

    return () => {
      alive = false;
      loadIdRef.current += 1;
      cancelPendingSeek();
      wantPlayRef.current = false;
      stopAtRef.current = null;
      listeners.forEach(([type, listener]) => audio.removeEventListener(type, listener));
      audio.pause();
      audio.removeAttribute('src');
      audio.load();
      dropNextClip();
      revoke(clipRef.current);
      clipRef.current = null;
      audioRef.current = null;
    };
  }, [enabled, meetingId, cancelPendingSeek, clock, dropNextClip, loadClipAt, renderClip, report, showClip]);

  // Space toggles playback unless focus is in a text field (the seek bar and buttons included).
  useEffect(() => {
    if (!source || typeof document === 'undefined') return;
    const onKeyDown = (event: KeyboardEvent) => {
      if ((event.code !== 'Space' && event.key !== ' ') || event.repeat || event.defaultPrevented) return;
      if (isTypingTarget(event.target as HTMLElement | null)) return;
      event.preventDefault();
      toggle();
    };
    document.addEventListener('keydown', onKeyDown);
    return () => document.removeEventListener('keydown', onKeyDown);
  }, [source, toggle]);

  return {
    ready: source !== null,
    error,
    playing,
    clock,
    durationS: source?.duration_s ?? 0,
    rate,
    playFrom,
    toggle,
    seek,
    setRate,
    refresh,
  };
}
