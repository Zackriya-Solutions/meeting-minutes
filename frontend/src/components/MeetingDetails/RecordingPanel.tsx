"use client";

import {
  forwardRef,
  useCallback,
  useEffect,
  useImperativeHandle,
  useRef,
  useState,
} from 'react';
import { motion } from 'framer-motion';
import { AudioLines, Pause, Play, RotateCcw, RotateCw } from 'lucide-react';
import { Button } from '@/components/ui/button';
import Analytics from '@/lib/analytics';

const SKIP_SECONDS = 10;
const PLAYBACK_RATES = [1, 1.25, 1.5, 2];

export interface RecordingPanelRef {
  /** Seek to a position (in seconds) and start playback. */
  seekTo: (seconds: number) => void;
}

interface RecordingPanelProps {
  /** Playable URL of the meeting recording (asset protocol). */
  audioSrc: string;
  meetingTitle: string;
}

// Format seconds as M:SS (or H:MM:SS for recordings over an hour)
function formatPlaybackTime(seconds: number): string {
  if (!isFinite(seconds) || seconds < 0) return '0:00';

  const totalSeconds = Math.floor(seconds);
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const secs = totalSeconds % 60;

  if (hours > 0) {
    return `${hours}:${minutes.toString().padStart(2, '0')}:${secs.toString().padStart(2, '0')}`;
  }
  return `${minutes}:${secs.toString().padStart(2, '0')}`;
}

export const RecordingPanel = forwardRef<RecordingPanelRef, RecordingPanelProps>(
  function RecordingPanel({ audioSrc, meetingTitle }, ref) {
    const audioRef = useRef<HTMLAudioElement>(null);
    // Seek requested before audio metadata finished loading
    const pendingSeekRef = useRef<number | null>(null);

    const [isPlaying, setIsPlaying] = useState(false);
    const [currentTime, setCurrentTime] = useState(0);
    const [duration, setDuration] = useState(0);
    const [playbackRate, setPlaybackRate] = useState(1);
    const [loadError, setLoadError] = useState(false);

    const playFrom = useCallback((seconds: number) => {
      const audio = audioRef.current;
      if (!audio) return;

      if (audio.readyState === 0) {
        // Metadata not loaded yet - apply once it is
        pendingSeekRef.current = seconds;
        return;
      }

      audio.currentTime = Math.max(0, Math.min(seconds, audio.duration || seconds));
      audio.play().catch((error) => {
        console.error('Failed to start playback:', error);
      });
    }, []);

    useImperativeHandle(ref, () => ({
      seekTo: (seconds: number) => {
        playFrom(seconds);
      },
    }), [playFrom]);

    // Reset player state when the recording source changes (meeting switch)
    useEffect(() => {
      setIsPlaying(false);
      setCurrentTime(0);
      setDuration(0);
      setLoadError(false);
      pendingSeekRef.current = null;
    }, [audioSrc]);

    const handleLoadedMetadata = () => {
      const audio = audioRef.current;
      if (!audio) return;

      setDuration(audio.duration);
      setLoadError(false);

      if (pendingSeekRef.current !== null) {
        const seconds = pendingSeekRef.current;
        pendingSeekRef.current = null;
        playFrom(seconds);
      }
    };

    const handleTogglePlayback = () => {
      const audio = audioRef.current;
      if (!audio) return;

      if (audio.paused) {
        Analytics.trackButtonClick('play_recording', 'meeting_details');
        audio.play().catch((error) => {
          console.error('Failed to start playback:', error);
        });
      } else {
        audio.pause();
      }
    };

    const handleSkip = (offsetSeconds: number) => {
      const audio = audioRef.current;
      if (!audio) return;
      audio.currentTime = Math.max(0, Math.min(audio.currentTime + offsetSeconds, audio.duration || 0));
    };

    const handleSeekInput = (value: number) => {
      const audio = audioRef.current;
      if (!audio) return;
      audio.currentTime = value;
      setCurrentTime(value);
    };

    const handleCyclePlaybackRate = () => {
      const audio = audioRef.current;
      if (!audio) return;

      const currentIndex = PLAYBACK_RATES.indexOf(playbackRate);
      const nextRate = PLAYBACK_RATES[(currentIndex + 1) % PLAYBACK_RATES.length];
      audio.playbackRate = nextRate;
      setPlaybackRate(nextRate);
    };

    if (loadError) {
      return (
        <div className="flex flex-1 items-center justify-center p-6">
          <div className="text-center text-gray-500">
            <AudioLines className="mx-auto mb-3 h-8 w-8 text-gray-300" />
            <p className="text-sm font-medium">Couldn&apos;t load the recording</p>
            <p className="text-xs mt-1 text-gray-400">
              The audio file may have been moved or is in an unsupported format
            </p>
          </div>
        </div>
      );
    }

    return (
      <motion.div
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        transition={{ duration: 0.2 }}
        className="flex flex-1 flex-col items-center justify-center p-6"
      >
        <audio
          ref={audioRef}
          src={audioSrc}
          preload="metadata"
          onLoadedMetadata={handleLoadedMetadata}
          onPlay={() => setIsPlaying(true)}
          onPause={() => setIsPlaying(false)}
          onEnded={() => setIsPlaying(false)}
          onTimeUpdate={() => setCurrentTime(audioRef.current?.currentTime ?? 0)}
          onError={() => setLoadError(true)}
        />

        <div className="w-full max-w-md">
          {/* Recording header */}
          <div className="flex flex-col items-center mb-8">
            <div className={`flex items-center justify-center w-16 h-16 rounded-full mb-4 ${isPlaying ? 'bg-blue-100' : 'bg-gray-100'}`}>
              <AudioLines className={`h-8 w-8 ${isPlaying ? 'text-blue-500' : 'text-gray-400'}`} />
            </div>
            <p className="text-sm font-medium text-gray-800 text-center break-words max-w-full">
              {meetingTitle}
            </p>
            <p className="text-xs text-gray-400 mt-1">Meeting recording</p>
          </div>

          {/* Seek bar */}
          <div className="mb-6">
            <input
              type="range"
              min={0}
              max={duration || 0}
              step={0.1}
              value={Math.min(currentTime, duration || 0)}
              onChange={(e) => handleSeekInput(Number(e.target.value))}
              disabled={!duration}
              aria-label="Seek recording"
              className="w-full h-1.5 cursor-pointer accent-blue-500 disabled:cursor-default"
            />
            <div className="flex justify-between mt-1 text-xs text-gray-500 tabular-nums">
              <span>{formatPlaybackTime(currentTime)}</span>
              <span>{formatPlaybackTime(duration)}</span>
            </div>
          </div>

          {/* Transport controls */}
          <div className="flex items-center justify-center gap-3">
            <Button
              variant="ghost"
              size="icon"
              onClick={() => handleSkip(-SKIP_SECONDS)}
              disabled={!duration}
              title={`Back ${SKIP_SECONDS} seconds`}
              aria-label={`Back ${SKIP_SECONDS} seconds`}
            >
              <RotateCcw className="h-5 w-5" />
            </Button>

            <button
              onClick={handleTogglePlayback}
              disabled={!duration}
              title={isPlaying ? 'Pause' : 'Play'}
              aria-label={isPlaying ? 'Pause recording' : 'Play recording'}
              className="flex items-center justify-center w-12 h-12 rounded-full bg-blue-500 text-white hover:bg-blue-600 transition-colors disabled:opacity-50 disabled:hover:bg-blue-500 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:ring-offset-2"
            >
              {isPlaying ? (
                <Pause className="h-5 w-5" />
              ) : (
                <Play className="h-5 w-5 ml-0.5" />
              )}
            </button>

            <Button
              variant="ghost"
              size="icon"
              onClick={() => handleSkip(SKIP_SECONDS)}
              disabled={!duration}
              title={`Forward ${SKIP_SECONDS} seconds`}
              aria-label={`Forward ${SKIP_SECONDS} seconds`}
            >
              <RotateCw className="h-5 w-5" />
            </Button>

            <Button
              variant="ghost"
              size="sm"
              onClick={handleCyclePlaybackRate}
              disabled={!duration}
              title="Playback speed"
              aria-label={`Playback speed ${playbackRate}x`}
              className="min-w-[52px] text-xs font-semibold text-gray-600 tabular-nums"
            >
              {playbackRate}x
            </Button>
          </div>

          {/* Discoverability hint for transcript sync */}
          <p className="text-xs text-gray-400 text-center mt-8">
            Tip: click a timestamp in the transcript to jump to that moment
          </p>
        </div>
      </motion.div>
    );
  }
);
