// VALUEOS WS3: speaker-label mapping.
//
// Reality today (see valueos/FEATURE-speakers.md — achieved level: c): the audio stack
// captures mic and system as separate, source-tagged streams but SUMS them into one mono
// signal BEFORE transcription, and the emitted segment `source` is a hardcoded "Audio". So no
// real speaker distinction reaches our layer, and we do NOT edit upstream to force one.
//
// This module is the single place that maps a segment's `source` → a display label. It is the
// seam for real "Me:" / "Other:" labels the moment an upstream change starts populating
// `source` with the capture DeviceType (mic vs system). Until then it is INERT: an unknown /
// "Audio" source resolves to no label, so the stored + uploaded transcript text is byte-for-byte
// identical to a plain join — no regression, no overpromising in the UI.

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
