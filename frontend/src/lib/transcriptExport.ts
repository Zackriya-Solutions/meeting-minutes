import { Transcript } from '@/types';

export type TranscriptExportFormat = 'txt' | 'vtt';

/**
 * Format a recording-relative offset (in seconds) as `MM:SS`, used for the
 * plain-text export. Mirrors the timestamp style of the "Copy transcript" feature.
 */
function formatClockTime(seconds: number): string {
  const totalSecs = Math.max(0, Math.floor(seconds));
  const mins = Math.floor(totalSecs / 60);
  const secs = totalSecs % 60;
  return `${mins.toString().padStart(2, '0')}:${secs.toString().padStart(2, '0')}`;
}

/**
 * Format a recording-relative offset (in seconds) as a WebVTT timestamp
 * (`HH:MM:SS.mmm`).
 */
function formatVttTime(seconds: number): string {
  const safe = Math.max(0, seconds);
  const totalMs = Math.round(safe * 1000);
  const ms = totalMs % 1000;
  const totalSecs = Math.floor(totalMs / 1000);
  const secs = totalSecs % 60;
  const mins = Math.floor(totalSecs / 60) % 60;
  const hrs = Math.floor(totalSecs / 3600);
  return (
    `${hrs.toString().padStart(2, '0')}:` +
    `${mins.toString().padStart(2, '0')}:` +
    `${secs.toString().padStart(2, '0')}.` +
    `${ms.toString().padStart(3, '0')}`
  );
}

/**
 * Plain-text export: a readable transcript with a title header and
 * recording-relative `[MM:SS]` timestamps when available.
 */
export function formatTranscriptTxt(
  transcripts: Transcript[],
  meetingTitle: string,
  createdAt?: string,
): string {
  const header = `Transcript: ${meetingTitle}`;
  const dateLine = createdAt
    ? `Date: ${new Date(createdAt).toLocaleString()}`
    : '';
  const divider = '='.repeat(Math.max(header.length, 12));

  const body = transcripts
    .map((t) => {
      const stamp =
        t.audio_start_time !== undefined
          ? `[${formatClockTime(t.audio_start_time)}] `
          : t.timestamp
            ? `[${t.timestamp}] `
            : '';
      return `${stamp}${t.text.trim()}`;
    })
    .join('\n');

  return [header, dateLine, divider, '', body, ''].filter((l) => l !== null).join('\n');
}

/**
 * WebVTT export: timed cues suitable for use as subtitles/captions.
 *
 * Cue boundaries use the recording-relative `audio_start_time` /
 * `audio_end_time`. When an end time is missing we fall back to the next
 * segment's start; legacy segments without any offset are placed sequentially
 * so the file stays valid.
 */
export function formatTranscriptVtt(transcripts: Transcript[]): string {
  const lines: string[] = ['WEBVTT', ''];
  let cursor = 0; // fallback running clock for legacy segments

  transcripts.forEach((t, i) => {
    const start = t.audio_start_time ?? cursor;
    const next = transcripts[i + 1];
    let end = t.audio_end_time ?? next?.audio_start_time ?? start + 2;
    // A cue must have a strictly positive duration.
    if (end <= start) {
      end = start + 2;
    }
    cursor = end;

    lines.push(`${formatVttTime(start)} --> ${formatVttTime(end)}`);
    lines.push(t.text.trim());
    lines.push('');
  });

  return lines.join('\n');
}

/**
 * Build a filesystem-safe default filename (without extension) from the meeting title.
 */
export function buildExportFileName(meetingTitle: string): string {
  const base = (meetingTitle || 'transcript')
    .trim()
    .replace(/[^a-z0-9\-_ ]/gi, '')
    .replace(/\s+/g, '_')
    .slice(0, 80);
  return base || 'transcript';
}
