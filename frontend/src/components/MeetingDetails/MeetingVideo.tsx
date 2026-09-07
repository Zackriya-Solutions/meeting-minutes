'use client';

import { convertFileSrc, invoke } from '@tauri-apps/api/core';
import {
  FolderOpen,
  Loader2,
  Play,
  RotateCcw,
  Save,
  Scissors,
  SkipBack,
  SkipForward,
  Video,
  X,
} from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';

function formatTime(seconds: number) {
  if (!Number.isFinite(seconds)) return '0:00';
  const minutes = Math.floor(seconds / 60);
  const remainder = Math.floor(seconds % 60);
  return `${minutes}:${remainder.toString().padStart(2, '0')}`;
}

export function MeetingVideo({
  folderPath,
  onOpenFolder,
}: {
  folderPath?: string | null;
  onOpenFolder?: () => Promise<void>;
}) {
  const videoRef = useRef<HTMLVideoElement>(null);
  const [videoPath, setVideoPath] = useState<string | null>(null);
  const [duration, setDuration] = useState(0);
  const [trimStart, setTrimStart] = useState(0);
  const [trimEnd, setTrimEnd] = useState(0);
  const [isEditing, setIsEditing] = useState(false);
  const [isSaving, setIsSaving] = useState(false);
  const [isPreviewing, setIsPreviewing] = useState(false);
  const [revision, setRevision] = useState(0);

  const isTrimmed = videoPath?.endsWith('video-trimmed.mp4') ?? false;

  useEffect(() => {
    let active = true;
    if (!folderPath) {
      setVideoPath(null);
      return;
    }
    invoke<string | null>('get_meeting_video_path', { folderPath })
      .then((path) => {
        if (active) setVideoPath(path);
      })
      .catch((error) => console.error('Failed to find meeting video', error));
    return () => {
      active = false;
    };
  }, [folderPath]);

  const resetSelection = (nextDuration = duration) => {
    setTrimStart(0);
    setTrimEnd(nextDuration);
    setIsPreviewing(false);
  };

  const setStartFromPlayhead = () => {
    const currentTime = videoRef.current?.currentTime ?? 0;
    setTrimStart(Math.min(currentTime, Math.max(0, trimEnd - 0.1)));
  };

  const setEndFromPlayhead = () => {
    const currentTime = videoRef.current?.currentTime ?? duration;
    setTrimEnd(Math.max(currentTime, Math.min(duration, trimStart + 0.1)));
  };

  const previewSelection = async () => {
    const video = videoRef.current;
    if (!video) return;
    video.currentTime = trimStart;
    try {
      setIsPreviewing(true);
      await video.play();
    } catch (error) {
      setIsPreviewing(false);
      toast.error('Could not preview the selection', { description: String(error) });
    }
  };

  const saveTrim = async () => {
    if (!folderPath) return;
    setIsSaving(true);
    try {
      const path = await invoke<string>('trim_meeting_video', {
        folderPath,
        startSeconds: trimStart,
        endSeconds: trimEnd,
      });
      setVideoPath(path);
      setRevision((value) => value + 1);
      setIsEditing(false);
      toast.success('Trimmed video saved', {
        description: 'The original recording is still available.',
      });
    } catch (error) {
      toast.error('Could not trim the video', { description: String(error) });
    } finally {
      setIsSaving(false);
    }
  };

  const restoreOriginal = async () => {
    if (!folderPath) return;
    setIsSaving(true);
    try {
      const path = await invoke<string>('restore_original_meeting_video', { folderPath });
      setVideoPath(path);
      setRevision((value) => value + 1);
      setIsEditing(false);
      toast.success('Original video restored');
    } catch (error) {
      toast.error('Could not restore the original', { description: String(error) });
    } finally {
      setIsSaving(false);
    }
  };

  if (!videoPath) return null;

  return (
    <section className="border-b border-gray-200 bg-white p-3">
      <div className="mb-2 flex items-center justify-between gap-2">
        <div className="flex min-w-0 items-center gap-2 text-xs font-medium text-gray-700">
          <Video size={15} className="shrink-0 text-blue-600" />
          <span className="truncate">{isTrimmed ? 'Trimmed meeting video' : 'Meeting video'}</span>
          {duration > 0 && <span className="text-gray-400">{formatTime(duration)}</span>}
        </div>
        <div className="flex shrink-0 items-center gap-1">
          {onOpenFolder && (
            <Button
              type="button"
              variant="ghost"
              size="icon"
              onClick={onOpenFolder}
              title="Open meeting folder"
            >
              <FolderOpen />
              <span className="sr-only">Open meeting folder</span>
            </Button>
          )}
          {isTrimmed && (
            <Button
              type="button"
              variant="ghost"
              size="sm"
              disabled={isSaving}
              onClick={restoreOriginal}
              title="Restore the untouched original video"
            >
              <RotateCcw />
              Restore
            </Button>
          )}
          <Button
            type="button"
            variant={isEditing ? 'secondary' : 'ghost'}
            size="sm"
            disabled={isSaving || duration <= 0}
            onClick={() => {
              resetSelection();
              setIsEditing((value) => !value);
            }}
            title="Trim meeting video"
          >
            <Scissors />
            Trim
          </Button>
        </div>
      </div>

      <video
        key={`${videoPath}-${revision}`}
        ref={videoRef}
        controls
        preload="metadata"
        className="aspect-video w-full rounded-lg bg-black"
        src={convertFileSrc(videoPath)}
        onLoadedMetadata={(event) => {
          const nextDuration = event.currentTarget.duration;
          setDuration(nextDuration);
          resetSelection(nextDuration);
        }}
        onTimeUpdate={(event) => {
          if (isPreviewing && event.currentTarget.currentTime >= trimEnd) {
            event.currentTarget.pause();
            setIsPreviewing(false);
          }
        }}
        onPause={() => setIsPreviewing(false)}
      />

      {isEditing && (
        <div className="mt-3 space-y-3 rounded-lg border border-gray-200 bg-gray-50 p-3">
          <div className="flex items-center justify-between text-xs">
            <span className="font-medium text-gray-700">Keep selection</span>
            <span className="tabular-nums text-gray-500">
              {formatTime(trimStart)} – {formatTime(trimEnd)}
              {' · '}
              {formatTime(Math.max(0, trimEnd - trimStart))}
            </span>
          </div>

          <label className="grid grid-cols-[42px_1fr_46px] items-center gap-2 text-[11px] text-gray-600">
            Start
            <input
              type="range"
              min={0}
              max={Math.max(0, duration)}
              step={0.1}
              value={trimStart}
              onChange={(event) => {
                const value = Number(event.target.value);
                setTrimStart(Math.min(value, Math.max(0, trimEnd - 0.1)));
              }}
              className="accent-blue-600"
            />
            <span className="text-right tabular-nums">{formatTime(trimStart)}</span>
          </label>

          <label className="grid grid-cols-[42px_1fr_46px] items-center gap-2 text-[11px] text-gray-600">
            End
            <input
              type="range"
              min={0}
              max={Math.max(0, duration)}
              step={0.1}
              value={trimEnd}
              onChange={(event) => {
                const value = Number(event.target.value);
                setTrimEnd(Math.max(value, Math.min(duration, trimStart + 0.1)));
              }}
              className="accent-blue-600"
            />
            <span className="text-right tabular-nums">{formatTime(trimEnd)}</span>
          </label>

          <div className="grid grid-cols-2 gap-2">
            <Button type="button" variant="outline" size="sm" onClick={setStartFromPlayhead}>
              <SkipBack />
              Start here
            </Button>
            <Button type="button" variant="outline" size="sm" onClick={setEndFromPlayhead}>
              <SkipForward />
              End here
            </Button>
          </div>

          <div className="flex flex-wrap justify-end gap-2">
            <Button type="button" variant="ghost" size="sm" onClick={() => setIsEditing(false)}>
              <X />
              Cancel
            </Button>
            <Button
              type="button"
              variant="outline"
              size="sm"
              disabled={isSaving}
              onClick={previewSelection}
            >
              <Play />
              Preview
            </Button>
            <Button
              type="button"
              variant="blue"
              size="sm"
              disabled={isSaving || trimEnd - trimStart < 0.1}
              onClick={saveTrim}
            >
              {isSaving ? <Loader2 className="animate-spin" /> : <Save />}
              Save trim
            </Button>
          </div>
        </div>
      )}
    </section>
  );
}
