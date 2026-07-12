import type { TranscriptPreview } from '@/types';

export function reduceTranscriptPreview(
  _current: TranscriptPreview | null,
  event: TranscriptPreview
): TranscriptPreview | null {
  const confirmedText = event.confirmed_text.trim();
  const text = event.text.trim();
  if (!confirmedText && !text) {
    return null;
  }

  return {
    ...event,
    confirmed_text: confirmedText,
    text,
  };
}
