import type { Transcript } from '@/types';

const DISPLAY_CONFIDENCE_THRESHOLD = 0.55;

export function transcriptSourceLabel(
  audioSource?: string | null,
  sourceConfidence?: number | null
): 'Me' | 'Speaker' | null {
  if (audioSource == null) {
    return null;
  }

  if (
    typeof sourceConfidence !== 'number'
    || !Number.isFinite(sourceConfidence)
    || sourceConfidence > 1
    || sourceConfidence < DISPLAY_CONFIDENCE_THRESHOLD
  ) {
    return 'Speaker';
  }

  switch (audioSource) {
    case 'microphone':
      return 'Me';
    case 'system':
      return 'Speaker';
    default:
      return 'Speaker';
  }
}

export function formatTranscriptLine(transcript: Transcript, formattedTime: string): string {
  const label = transcriptSourceLabel(transcript.audio_source, transcript.source_confidence);
  return label
    ? `${formattedTime} ${label}: ${transcript.text}`
    : `${formattedTime} ${transcript.text}`;
}
