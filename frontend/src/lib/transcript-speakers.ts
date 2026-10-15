// Audio-source labels carried on transcript segments (`speaker`).
//
// DO NOT edit these values here alone: they MUST equal MIC_IN_LABEL and
// SPEAKER_OUT_LABEL in src-tauri/src/audio/deepgram.rs, which produces them.
// A Rust unit test (`ts_speaker_labels_match_rust_constants`) reads this file
// and fails if the two drift or if another *_LABEL constant appears.
export const MIC_IN_LABEL = 'Mic-In';
export const SPEAKER_OUT_LABEL = 'Speaker-Out';

/**
 * Tailwind classes for a speaker label; unknown labels get neutral styling.
 * With diarization on, a label carries a 1-based voice number after the
 * channel ("Speaker-Out 2"), so match the channel prefix.
 */
export function speakerLabelClass(speaker: string): string {
  const channel = speaker.replace(/ \d+$/, '');
  switch (channel) {
    case MIC_IN_LABEL:
      return 'text-blue-600';
    case SPEAKER_OUT_LABEL:
      return 'text-emerald-600';
    default:
      return 'text-gray-500';
  }
}

/** One transcript line for copy/export/summary input: "Mic-In: text" when labelled. */
export function formatTranscriptLine(text: string, speaker?: string | null): string {
  return speaker ? `${speaker}: ${text}` : text;
}
