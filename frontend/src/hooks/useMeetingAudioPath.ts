import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';

export interface UseMeetingAudioPathResult {
  /** Resolved absolute audio path, or null if not available. */
  audioPath: string | null;
  /** True while the backend lookup is in flight. */
  loading: boolean;
  /** Error message from the backend, or null. */
  error: string | null;
}

/**
 * Resolves the local audio file path for a meeting by calling the
 * `get_meeting_audio_path` Tauri command (Wave 14 PR-44d).
 *
 * The hook returns `null` for the path while loading or when the meeting
 * has no browser-decodable audio (legacy mp4-only meetings until PR-44e
 * lands a parallel WAV export). Components should treat `null` as
 * "feature unavailable" and gracefully hide audio UI.
 */
export function useMeetingAudioPath(
  meetingId: string | null | undefined,
): UseMeetingAudioPathResult {
  const [audioPath, setAudioPath] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!meetingId) {
      setAudioPath(null);
      setLoading(false);
      setError(null);
      return;
    }

    let cancelled = false;
    setLoading(true);
    setError(null);

    invoke<string | null>('get_meeting_audio_path', { meetingId })
      .then((path) => {
        if (cancelled) return;
        setAudioPath(path ?? null);
        setLoading(false);
      })
      .catch((err: unknown) => {
        if (cancelled) return;
        const msg = err instanceof Error ? err.message : String(err);
        setError(msg);
        setAudioPath(null);
        setLoading(false);
      });

    return () => {
      cancelled = true;
    };
  }, [meetingId]);

  return { audioPath, loading, error };
}
