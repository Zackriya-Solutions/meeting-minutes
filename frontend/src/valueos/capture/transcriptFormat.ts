// VALUEOS WS1: turn transcription segments into a readable, labelled, timestamped transcript.
// One line per utterance — "[HH:MM:SS] Label: text" — with a small header (start time, the
// timestamp basis, and the speaker labels used). Speaker labels come from each segment's
// `source` (WS2: Me / Other, best-effort). A segment with no speaker signal still renders with
// its timestamp and a neutral fallback label — never an invented speaker.
//
// This single formatter is used for BOTH the .txt written to the configured folder AND the
// raw_content uploaded to ValueOS, so the two always match.
import { sourceToLabel } from './speakerLabels';

export interface TranscriptSegment {
  text: string;
  /** 'Me' | 'Other' | … from the transcript-update event (WS2). Absent → fallback label. */
  source?: string | null;
  audio_start_time?: number; // seconds from recording start
  audio_end_time?: number; // seconds from recording start
  is_partial?: boolean;
  sequence_id?: number;
}

export interface FormatOptions {
  /** Wall-clock ms at recording start — recorded in the header when present. */
  startedAt?: number;
  /** Label for segments with no speaker signal (default "Speaker"). */
  fallbackLabel?: string;
  /** Emit the header block (default true). */
  header?: boolean;
}

function pad(n: number): string {
  return n.toString().padStart(2, '0');
}

/** Elapsed seconds → HH:MM:SS. */
export function hhmmss(totalSeconds: number): string {
  const s = Math.max(0, Math.floor(totalSeconds || 0));
  return `${pad(Math.floor(s / 3600))}:${pad(Math.floor((s % 3600) / 60))}:${pad(s % 60)}`;
}

/** Elapsed seconds → HH:MM:SS.mmm (WebVTT cue time). */
function vttTime(totalSeconds: number): string {
  const t = Math.max(0, totalSeconds || 0);
  const ms = Math.floor((t - Math.floor(t)) * 1000);
  return `${hhmmss(t)}.${ms.toString().padStart(3, '0')}`;
}

/** Confirmed (final), non-empty segments, ordered by start time then sequence. */
function ordered(segments: TranscriptSegment[]): TranscriptSegment[] {
  const seen = new Set<string>();
  return segments
    .filter((s) => s.is_partial !== true && (s.text ?? '').trim().length > 0)
    .filter((s) => {
      // de-dup exact repeats (same sequence + text)
      const key = `${s.sequence_id ?? ''}|${s.text.trim()}`;
      if (seen.has(key)) return false;
      seen.add(key);
      return true;
    })
    .slice()
    .sort(
      (a, b) =>
        (a.audio_start_time ?? 0) - (b.audio_start_time ?? 0) ||
        (a.sequence_id ?? 0) - (b.sequence_id ?? 0),
    );
}

/** The plain-text labelled/timestamped transcript. */
export function formatTranscript(segments: TranscriptSegment[], opts: FormatOptions = {}): string {
  const fallback = opts.fallbackLabel ?? 'Speaker';
  const finals = ordered(segments);
  const labels = new Set<string>();

  const lines = finals.map((s) => {
    const label = sourceToLabel(s.source) ?? fallback;
    labels.add(label);
    return `[${hhmmss(s.audio_start_time ?? 0)}] ${label}: ${s.text.trim()}`;
  });

  const body = lines.join('\n');
  if (opts.header === false) return body ? `${body}\n` : '';

  const head: string[] = ['# ValueOS Agent transcript'];
  if (opts.startedAt) head.push(`# Started: ${new Date(opts.startedAt).toISOString()}`);
  head.push('# Timestamps: elapsed from recording start (HH:MM:SS)');
  if (labels.size) head.push(`# Speakers: ${Array.from(labels).join(', ')}`);
  return `${head.join('\n')}\n\n${body}\n`;
}

/** Optional WebVTT rendering (API content_type text/vtt). Timed segments map naturally to cues. */
export function formatVtt(segments: TranscriptSegment[], opts: FormatOptions = {}): string {
  const fallback = opts.fallbackLabel ?? 'Speaker';
  const finals = ordered(segments);
  const cues = finals.map((s, i) => {
    const start = s.audio_start_time ?? 0;
    const end = s.audio_end_time ?? start + 2;
    const label = sourceToLabel(s.source) ?? fallback;
    return `${i + 1}\n${vttTime(start)} --> ${vttTime(end)}\n<v ${label}>${s.text.trim()}`;
  });
  return `WEBVTT\n\n${cues.join('\n\n')}\n`;
}
