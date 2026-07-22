import { useCallback, useEffect, useRef, useState } from 'react';

/**
 * How long we wait for a requested playback to actually start (or resume) before
 * treating the current source as stalled and falling back to the next one. A
 * media element that never buffers fires neither `playing` nor `error`, so
 * without this the play button would spin forever.
 */
const STALL_TIMEOUT_MS = 8_000;

/**
 * Streaming player for a recording exposed through Tauri's asset protocol.
 *
 * Deliberately uses the browser's native HTMLMediaElement pipeline. The old
 * implementation returned every byte through JSON IPC and decoded the whole
 * recording in an AudioBuffer; a 1 GB recording could therefore consume tens
 * of gigabytes in the webview. Native media loading supports byte ranges,
 * seeking and constant-memory playback.
 */
export const useAudioPlayer = (audioUrls: string | string[] | null) => {
  const [isPlaying, setIsPlaying] = useState(false);
  const [isLoading, setIsLoading] = useState(false);
  const [currentTime, setCurrentTime] = useState(0);
  const [duration, setDuration] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const elementRef = useRef<HTMLAudioElement | null>(null);
  const wantsPlaybackRef = useRef(false);
  const requestedTimeRef = useRef(0);
  // Bridges the stall watchdog (owned by the effect below) to the play/pause/stop
  // callbacks, which live outside the effect closure.
  const armWatchdogRef = useRef<() => void>(() => {});
  const clearWatchdogRef = useRef<() => void>(() => {});
  const sourceKey = Array.isArray(audioUrls) ? audioUrls.join('\n') : (audioUrls || '');

  useEffect(() => {
    const previous = elementRef.current;
    if (previous) {
      previous.pause();
      previous.removeAttribute('src');
      previous.load();
    }

    setIsPlaying(false);
    setIsLoading(false);
    setCurrentTime(0);
    setDuration(0);
    setError(null);
    const sources = (Array.isArray(audioUrls) ? audioUrls : audioUrls ? [audioUrls] : [])
      .filter((value, index, values) => Boolean(value) && values.indexOf(value) === index);
    if (sources.length === 0) {
      elementRef.current = null;
      return;
    }

    const element = new Audio();
    element.preload = 'metadata';
    let sourceIndex = 0;
    let switchingSource = false;
    element.src = sources[sourceIndex];
    elementRef.current = element;

    // Watchdog for silent stalls (see STALL_TIMEOUT_MS). Some macOS WKWebView
    // builds never buffer the asset-protocol source and emit no `error` event, so
    // we time out and advance to the next source ourselves.
    let stallTimer: ReturnType<typeof setTimeout> | null = null;
    let stallAnchor = 0;
    const clearStallTimer = () => {
      if (stallTimer !== null) {
        clearTimeout(stallTimer);
        stallTimer = null;
      }
    };
    const armStallTimer = () => {
      clearStallTimer();
      stallAnchor = Number.isFinite(element.currentTime) ? element.currentTime : 0;
      stallTimer = setTimeout(handleStall, STALL_TIMEOUT_MS);
    };

    // Try the next source in the ordered fallback list. Returns false when the
    // list is exhausted.
    const advanceSource = () => {
      if (sourceIndex + 1 >= sources.length) return false;
      sourceIndex += 1;
      switchingSource = true;
      setError(null);
      setIsLoading(true);
      element.src = sources[sourceIndex];
      element.load();
      if (wantsPlaybackRef.current) {
        armStallTimer();
        void element.play().catch((fallbackError) => {
          console.warn('Fallback meeting audio source did not start immediately:', fallbackError);
        });
      }
      return true;
    };
    const failPlayback = (message: string) => {
      clearStallTimer();
      wantsPlaybackRef.current = false;
      setIsPlaying(false);
      setIsLoading(false);
      setError(message);
    };
    function handleStall() {
      stallTimer = null;
      if (!wantsPlaybackRef.current) return;
      const progressed = Number.isFinite(element.currentTime) && element.currentTime > stallAnchor + 0.1;
      if (progressed && !element.paused) return; // playback is actually advancing
      console.warn('Meeting audio source stalled; attempting recovery', { source: sources[sourceIndex] });
      if (advanceSource()) return;
      failPlayback('Audio playback timed out. Please try again.');
    }

    const updateTime = () => {
      const value = Number.isFinite(element.currentTime) ? element.currentTime : 0;
      if (!switchingSource) requestedTimeRef.current = value;
      setCurrentTime(value);
    };
    const updateDuration = () => setDuration(Number.isFinite(element.duration) ? element.duration : 0);
    const markWaiting = () => {
      setIsLoading(true);
      if (wantsPlaybackRef.current) armStallTimer();
    };
    const markReady = () => {
      setIsLoading(false);
      updateDuration();
      if (requestedTimeRef.current > 0 && Math.abs(element.currentTime - requestedTimeRef.current) > 0.25) {
        const upperBound = Number.isFinite(element.duration) ? element.duration : requestedTimeRef.current;
        element.currentTime = Math.min(requestedTimeRef.current, upperBound);
      }
      switchingSource = false;
    };
    // A scrub sets currentTime, which fires `seeking` (arming the loading
    // spinner). When the seek resolves onto already-buffered data the readyState
    // never drops, so `canplay`/`playing` do not re-fire — reconcile the loading
    // state here or the spinner would spin forever after fast-forwarding.
    const markSeeked = () => {
      const ready = element.readyState >= HTMLMediaElement.HAVE_FUTURE_DATA;
      setIsLoading(!ready);
      if (ready) clearStallTimer();
    };
    const markPlaying = () => {
      clearStallTimer();
      setIsPlaying(true);
      setIsLoading(false);
      setError(null);
    };
    // Only drop the watchdog when the user actually stopped wanting playback;
    // switching sources emits a `pause` we must not treat as user intent.
    const markPaused = () => {
      if (!wantsPlaybackRef.current) clearStallTimer();
      setIsPlaying(false);
    };
    const markEnded = () => {
      clearStallTimer();
      setIsPlaying(false);
      updateTime();
    };
    const markError = () => {
      const mediaError = element.error;
      console.warn('Meeting audio source failed', {
        source: sources[sourceIndex],
        code: mediaError?.code,
        message: mediaError?.message,
      });
      if (advanceSource()) return;
      failPlayback('Failed to play audio recording');
    };

    element.addEventListener('timeupdate', updateTime);
    element.addEventListener('durationchange', updateDuration);
    element.addEventListener('loadedmetadata', markReady);
    element.addEventListener('canplay', markReady);
    element.addEventListener('waiting', markWaiting);
    element.addEventListener('seeking', markWaiting);
    element.addEventListener('seeked', markSeeked);
    element.addEventListener('playing', markPlaying);
    element.addEventListener('pause', markPaused);
    element.addEventListener('ended', markEnded);
    element.addEventListener('error', markError);
    element.load();

    armWatchdogRef.current = () => {
      if (wantsPlaybackRef.current) armStallTimer();
    };
    clearWatchdogRef.current = clearStallTimer;

    return () => {
      clearStallTimer();
      armWatchdogRef.current = () => {};
      clearWatchdogRef.current = () => {};
      element.pause();
      element.removeEventListener('timeupdate', updateTime);
      element.removeEventListener('durationchange', updateDuration);
      element.removeEventListener('loadedmetadata', markReady);
      element.removeEventListener('canplay', markReady);
      element.removeEventListener('waiting', markWaiting);
      element.removeEventListener('seeking', markWaiting);
      element.removeEventListener('seeked', markSeeked);
      element.removeEventListener('playing', markPlaying);
      element.removeEventListener('pause', markPaused);
      element.removeEventListener('ended', markEnded);
      element.removeEventListener('error', markError);
      element.removeAttribute('src');
      element.load();
      wantsPlaybackRef.current = false;
      requestedTimeRef.current = 0;
      if (elementRef.current === element) elementRef.current = null;
    };
  // sourceKey intentionally represents the ordered source list. Depending on the
  // array identity would recreate the native element on every render.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sourceKey]);

  const play = useCallback(async () => {
    const element = elementRef.current;
    if (!element) return;
    wantsPlaybackRef.current = true;
    try {
      if (Number.isFinite(element.duration) && element.currentTime >= element.duration) {
        element.currentTime = 0;
      }
      setIsLoading(element.readyState < HTMLMediaElement.HAVE_FUTURE_DATA);
      armWatchdogRef.current();
      await element.play();
    } catch (playError) {
      // A new load()/pause() — including our own source fallback — rejects the
      // pending play() promise with AbortError. That is expected and handled by
      // the media event listeners, so it must not surface an error or disarm the
      // stall watchdog for the source we just switched to.
      if (playError instanceof DOMException && playError.name === 'AbortError') return;
      console.error('Native audio playback failed:', playError);
      clearWatchdogRef.current();
      setIsLoading(false);
      // DOMException messages are browser/OS strings (for example, "The operation is not
      // supported") and therefore cannot be translated reliably in the UI.
      setError('Failed to play audio recording');
    }
  }, []);

  const pause = useCallback(() => {
    wantsPlaybackRef.current = false;
    clearWatchdogRef.current();
    elementRef.current?.pause();
  }, []);

  const seek = useCallback(async (time: number) => {
    const element = elementRef.current;
    if (!element) return;
    const upperBound = Number.isFinite(element.duration) ? element.duration : Number.MAX_SAFE_INTEGER;
    element.currentTime = Math.min(Math.max(time, 0), upperBound);
    requestedTimeRef.current = element.currentTime;
    setCurrentTime(element.currentTime);
  }, []);

  const playFrom = useCallback(async (time: number) => {
    await seek(time);
    await play();
  }, [play, seek]);

  return {
    isPlaying,
    isLoading,
    currentTime,
    duration,
    error,
    play,
    playFrom,
    pause,
    seek,
  };
};
