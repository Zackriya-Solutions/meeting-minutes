// VALUEOS WS3: speaker-label mapping.
//
// The native pipeline now tags each emitted segment's `source` with a BEST-EFFORT speaker
// (see valueos/FEATURE-speakers.md and pipeline.rs `valueos_attribute`): it compares the RAW
// system energy for the utterance against the RAW mic level (captured BEFORE loudness
// normalization). System-dominant → the remote/video was playing ("Other"); otherwise the
// speech came from the local mic ("Me").
//
// This module is the single place that maps a segment's `source` → a display label. A source
// it can't resolve (or none at all) still yields NO label — so the stored + uploaded text
// degrades gracefully to a plain join rather than inventing a speaker.

export interface LabeledSegment {
  text: string;
  /** Raw per-segment source, if the stack ever provides one (mic/system/speaker N). Today the
   *  transcript segments carry no source, so this is undefined and no label is produced. */
  source?: string | null;
}

/**
 * Map a raw segment source to a stable display label, or null when it carries no usable
 * speaker signal (the current reality — the stack emits a constant "Audio").
 */
export function sourceToLabel(source?: string | null): string | null {
  if (!source) return null;
  const s = source.trim().toLowerCase();
  if (s === 'microphone' || s === 'mic' || s === 'me') return 'Me';
  if (s === 'system' || s === 'speaker' || s === 'other' || s === 'received') return 'Other';
  const numbered = s.match(/^speaker[\s_-]*(\d+)$/);
  if (numbered) return `Speaker ${numbered[1]}`;
  return null; // "audio", unknown, empty → no label (inert today)
}

/**
 * Join segments into transcript text, prefixing each line with its speaker label when one can
 * be resolved. When NO segment resolves a label (today), the output is identical to a plain
 * text join — so the uploaded raw_content and the stored .txt are unchanged until real labels
 * exist. When labels do resolve later, they appear in BOTH (this is the single join point used
 * for the live view, the stored file, and the upload).
 */
export function labelTranscript(segments: LabeledSegment[]): string {
  return segments
    .map((seg) => {
      const text = seg.text?.trim();
      if (!text) return '';
      const label = sourceToLabel(seg.source);
      return label ? `${label}: ${text}` : text;
    })
    .filter(Boolean)
    .join('\n');
}
