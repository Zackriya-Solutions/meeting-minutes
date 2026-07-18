import { describe, it, expect } from 'vitest';
import { formatTranscript, formatVtt, hhmmss } from '@/valueos/capture/transcriptFormat';
import type { TranscriptSegment } from '@/valueos/capture/transcriptFormat';

// VALUEOS WS1: the formatter turns (mocked) segments → the exact labelled/timestamped layout.
const seg = (over: Partial<TranscriptSegment>): TranscriptSegment => ({ text: 'x', is_partial: false, ...over });

describe('formatTranscript', () => {
  it('renders one "[HH:MM:SS] Label: text" line per utterance, in time order', () => {
    const out = formatTranscript(
      [
        seg({ text: 'Thanks for making the time today.', source: 'Me', audio_start_time: 4, sequence_id: 1 }),
        seg({ text: 'Of course — happy to walk through it.', source: 'Other', audio_start_time: 9, sequence_id: 2 }),
        seg({ text: "Great, let's start with the pipeline.", source: 'Me', audio_start_time: 75, sequence_id: 3 }),
      ],
      { header: false },
    );
    expect(out).toBe(
      '[00:00:04] Me: Thanks for making the time today.\n' +
        '[00:00:09] Other: Of course — happy to walk through it.\n' +
        "[00:01:15] Me: Great, let's start with the pipeline.\n",
    );
  });

  it('maps mic→Me and system→Other source values', () => {
    const out = formatTranscript(
      [
        seg({ text: 'a', source: 'mic', audio_start_time: 0, sequence_id: 1 }),
        seg({ text: 'b', source: 'system', audio_start_time: 1, sequence_id: 2 }),
      ],
      { header: false },
    );
    expect(out).toContain('Me: a');
    expect(out).toContain('Other: b');
  });

  it('falls back to a neutral label (never an invented speaker) but keeps the timestamp', () => {
    const out = formatTranscript([seg({ text: 'hello', audio_start_time: 2, sequence_id: 1 })], { header: false });
    expect(out).toBe('[00:00:02] Speaker: hello\n');
  });

  it('drops partials + blanks, de-dups exact repeats, sorts by time', () => {
    const out = formatTranscript(
      [
        seg({ text: 'second', source: 'Me', audio_start_time: 5, sequence_id: 2 }),
        seg({ text: 'still typing', is_partial: true, audio_start_time: 1, sequence_id: 9, source: 'Me' }),
        seg({ text: '   ', audio_start_time: 2, sequence_id: 3 }),
        seg({ text: 'first', source: 'Me', audio_start_time: 1, sequence_id: 1 }),
        seg({ text: 'first', source: 'Me', audio_start_time: 1, sequence_id: 1 }), // duplicate
      ],
      { header: false },
    );
    expect(out).toBe('[00:00:01] Me: first\n[00:00:05] Me: second\n');
  });

  it('header records the start time, the elapsed basis, and the speakers used', () => {
    const out = formatTranscript([seg({ text: 'hi', source: 'Me', audio_start_time: 0, sequence_id: 1 })], {
      startedAt: Date.parse('2026-07-18T10:00:00Z'),
    });
    expect(out).toContain('# ValueOS Agent transcript');
    expect(out).toContain('# Started: 2026-07-18T10:00:00.000Z');
    expect(out).toContain('# Timestamps: elapsed from recording start (HH:MM:SS)');
    expect(out).toContain('# Speakers: Me');
    expect(out).toContain('[00:00:00] Me: hi');
  });
});

describe('hhmmss + formatVtt', () => {
  it('formats elapsed seconds', () => {
    expect(hhmmss(0)).toBe('00:00:00');
    expect(hhmmss(75)).toBe('00:01:15');
    expect(hhmmss(3661)).toBe('01:01:01');
  });

  it('emits WEBVTT cues with speaker voice tags', () => {
    const vtt = formatVtt([seg({ text: 'hi', source: 'Me', audio_start_time: 1, audio_end_time: 2, sequence_id: 1 })]);
    expect(vtt.startsWith('WEBVTT')).toBe(true);
    expect(vtt).toContain('00:00:01.000 --> 00:00:02.000');
    expect(vtt).toContain('<v Me>hi');
  });
});
