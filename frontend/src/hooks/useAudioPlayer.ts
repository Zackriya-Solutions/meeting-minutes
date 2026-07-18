import { useCallback, useEffect, useRef, useState } from 'react';

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

    const updateTime = () => {
      const value = Number.isFinite(element.currentTime) ? element.currentTime : 0;
      if (!switchingSource) requestedTimeRef.current = value;
      setCurrentTime(value);
    };
    const updateDuration = () => setDuration(Number.isFinite(element.duration) ? element.duration : 0);
    const markWaiting = () => setIsLoading(true);
    const markReady = () => {
      setIsLoading(false);
      updateDuration();
      if (requestedTimeRef.current > 0 && Math.abs(element.currentTime - requestedTimeRef.current) > 0.25) {
        const upperBound = Number.isFinite(element.duration) ? element.duration : requestedTimeRef.current;
        element.currentTime = Math.min(requestedTimeRef.current, upperBound);
      }
      switchingSource = false;
    };
    const markPlaying = () => {
      setIsPlaying(true);
      setIsLoading(false);
      setError(null);
    };
    const markPaused = () => setIsPlaying(false);
    const markEnded = () => {
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
      if (sourceIndex + 1 < sources.length) {
        sourceIndex += 1;
        switchingSource = true;
        setError(null);
        setIsLoading(true);
        element.src = sources[sourceIndex];
        element.load();
        if (wantsPlaybackRef.current) {
          void element.play().catch((fallbackError) => {
            console.warn('Fallback meeting audio source did not start immediately:', fallbackError);
          });
        }
        return;
      }
      setIsPlaying(false);
      setIsLoading(false);
      setError('Failed to play audio recording');
    };

    element.addEventListener('timeupdate', updateTime);
    element.addEventListener('durationchange', updateDuration);
    element.addEventListener('loadedmetadata', markReady);
    element.addEventListener('canplay', markReady);
    element.addEventListener('waiting', markWaiting);
    element.addEventListener('seeking', markWaiting);
    element.addEventListener('playing', markPlaying);
    element.addEventListener('pause', markPaused);
    element.addEventListener('ended', markEnded);
    element.addEventListener('error', markError);
    element.load();

    return () => {
      element.pause();
      element.removeEventListener('timeupdate', updateTime);
      element.removeEventListener('durationchange', updateDuration);
      element.removeEventListener('loadedmetadata', markReady);
      element.removeEventListener('canplay', markReady);
      element.removeEventListener('waiting', markWaiting);
      element.removeEventListener('seeking', markWaiting);
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
      await element.play();
    } catch (playError) {
      console.error('Native audio playback failed:', playError);
      setIsLoading(false);
      // DOMException messages are browser/OS strings (for example, "The operation is not
      // supported") and therefore cannot be translated reliably in the UI.
      setError('Failed to play audio recording');
    }
  }, []);

  const pause = useCallback(() => {
    wantsPlaybackRef.current = false;
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
