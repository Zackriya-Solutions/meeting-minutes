import type { TranscriptPreview } from '@/types';

export function reduceTranscriptPreview(
  _current: TranscriptPreview | null,
  event: TranscriptPreview
): TranscriptPreview | null {
  const text = event.text.trim();
  if (!text) {
    return null;
  }

  return {
    ...event,
    text,
  };
}
