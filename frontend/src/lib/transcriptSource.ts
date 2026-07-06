import type { Transcript } from '@/types';

const DISPLAY_CONFIDENCE_THRESHOLD = 0.55;

/**
 * Map a transcript's raw attribution to the BINARY presentation label.
 *
 * This is intentionally lossy and mirrors the backend `display_label`: only a
 * confident `microphone` becomes "Me"; `system`, `mixed`, `unknown`, and any
 * source below {@link DISPLAY_CONFIDENCE_THRESHOLD} all become "Speaker". The
 * goal is to never assert a false "Me", so "Speaker" doubles as the safe
 * fallback. Returns `null` when there is no attribution at all (`audio_source`
 * missing/NULL — e.g. pre-feature, imported, or re-transcribed segments), so the
 * UI can render no badge and copy/summary output can omit the label rather than
 * fabricating one. Callers needing the underlying four-state signal must read
 * `audio_source` / `source_confidence` directly (see {@link transcriptSourceTooltip}).
 */
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

/**
 * Human-readable raw attribution for the badge tooltip: the underlying
 * four-state `audio_source` plus confidence, e.g. "Source: microphone (91%
 * confidence)". Unlike {@link transcriptSourceLabel} this does NOT collapse the
 * state, so a hover reveals whether a "Speaker" badge was actually system,
 * mixed, unknown, or a low-confidence microphone. Returns `null` when there is
 * no attribution, so no tooltip is attached.
 */
export function transcriptSourceTooltip(
  audioSource?: string | null,
  sourceConfidence?: number | null
): string | null {
  if (audioSource == null) {
    return null;
  }

  const source = audioSource.charAt(0).toUpperCase() + audioSource.slice(1);
  if (typeof sourceConfidence === 'number' && Number.isFinite(sourceConfidence)) {
    const pct = Math.round(Math.max(0, Math.min(1, sourceConfidence)) * 100);
    return `Source: ${source} (${pct}% confidence)`;
  }
  return `Source: ${source}`;
}

export function formatTranscriptLine(transcript: Transcript, formattedTime: string): string {
  const label = transcriptSourceLabel(transcript.audio_source, transcript.source_confidence);
  return label
    ? `${formattedTime} ${label}: ${transcript.text}`
    : `${formattedTime} ${transcript.text}`;
}
