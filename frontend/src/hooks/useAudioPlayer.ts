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
export const useAudioPlayer = (audioUrl: string | null) => {
  const [isPlaying, setIsPlaying] = useState(false);
  const [isLoading, setIsLoading] = useState(false);
  const [currentTime, setCurrentTime] = useState(0);
  const [duration, setDuration] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const elementRef = useRef<HTMLAudioElement | null>(null);

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
    if (!audioUrl) {
      elementRef.current = null;
      return;
    }

    const element = new Audio();
    element.preload = 'metadata';
    element.src = audioUrl;
    elementRef.current = element;

    const updateTime = () => setCurrentTime(Number.isFinite(element.currentTime) ? element.currentTime : 0);
    const updateDuration = () => setDuration(Number.isFinite(element.duration) ? element.duration : 0);
    const markWaiting = () => setIsLoading(true);
    const markReady = () => {
      setIsLoading(false);
      updateDuration();
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
      if (elementRef.current === element) elementRef.current = null;
    };
  }, [audioUrl]);

  const play = useCallback(async () => {
    const element = elementRef.current;
    if (!element) return;
    try {
      if (Number.isFinite(element.duration) && element.currentTime >= element.duration) {
        element.currentTime = 0;
      }
      setIsLoading(element.readyState < HTMLMediaElement.HAVE_FUTURE_DATA);
      await element.play();
    } catch (playError) {
      setIsLoading(false);
      setError(playError instanceof Error ? playError.message : 'Failed to play audio recording');
    }
  }, []);

  const pause = useCallback(() => {
    elementRef.current?.pause();
  }, []);

  const seek = useCallback(async (time: number) => {
    const element = elementRef.current;
    if (!element) return;
    const upperBound = Number.isFinite(element.duration) ? element.duration : Number.MAX_SAFE_INTEGER;
    element.currentTime = Math.min(Math.max(time, 0), upperBound);
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
