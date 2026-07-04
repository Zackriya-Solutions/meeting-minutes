import { useEffect, useState } from 'react';
import { invoke, convertFileSrc } from '@tauri-apps/api/core';

interface UseMeetingAudioProps {
  meetingId: string;
  meetingFolderPath?: string | null;
}

/**
 * Locates the stored audio recording for a meeting and exposes a playable URL.
 *
 * Resolution happens in the Rust backend (get_meeting_audio_path), which also
 * adds the file to the asset protocol scope so the webview can stream it.
 * Returns a null audioSrc for meetings without a recording on disk
 * (e.g. legacy/folderless meetings), letting callers hide playback UI.
 */
export function useMeetingAudio({ meetingId, meetingFolderPath }: UseMeetingAudioProps) {
  const [audioSrc, setAudioSrc] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState<boolean>(false);

  useEffect(() => {
    let cancelled = false;
    setAudioSrc(null);

    if (!meetingFolderPath) {
      return;
    }

    const locateAudio = async () => {
      setIsLoading(true);
      try {
        const audioPath = await invoke<string | null>('get_meeting_audio_path', {
          meetingFolderPath,
        });

        if (!cancelled) {
          setAudioSrc(audioPath ? convertFileSrc(audioPath) : null);
        }
      } catch (error) {
        console.error('Failed to locate meeting audio for playback:', error);
        if (!cancelled) {
          setAudioSrc(null);
        }
      } finally {
        if (!cancelled) {
          setIsLoading(false);
        }
      }
    };

    locateAudio();

    return () => {
      cancelled = true;
    };
  }, [meetingId, meetingFolderPath]);

  return {
    audioSrc,
    isAudioAvailable: !!audioSrc,
    isLoading,
  };
}
