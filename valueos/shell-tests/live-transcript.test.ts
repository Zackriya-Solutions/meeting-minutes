import { describe, it, expect } from 'vitest';
import { reduceLive, deriveLive, recognitionActivity, emptyLive, toLiveLines } from '@/valueos/capture/liveTranscript';
import { rmsFromTimeDomain } from '@/valueos/capture/useMicLevel';
import { formatTranscript } from '@/valueos/capture/transcriptFormat';

// VALUEOS: live in-progress transcription — the interim state model + the fallback activity
// signal. Engine is mocked (plain update objects).

describe('toLiveLines — speaker-aligned chat lines', () => {
  it('maps "Me" → You (left/me) and "Other" → Other (right)', () => {
    const lines = toLiveLines({
      committed: [
        { text: 'This is me talking', source: 'Me' },
        { text: 'Immigration is a hot topic', source: 'Other' },
      ],
      interim: null,
    });
    expect(lines).toEqual([
      { role: 'me', label: 'You', text: 'This is me talking', partial: false },
      { role: 'other', label: 'Other', text: 'Immigration is a hot topic', partial: false },
    ]);
  });

  it('merges consecutive same-speaker committed segments into one bubble', () => {
    const lines = toLiveLines({
      committed: [
        { text: "Let's try to talk.", source: 'Me' },
        { text: 'So this is me talking.', source: 'Me' },
        { text: 'Someone else now.', source: 'Other' },
      ],
      interim: null,
    });
    expect(lines.map((l) => l.role)).toEqual(['me', 'other']);
    expect(lines[0].text).toBe("Let's try to talk. So this is me talking.");
  });

  it('renders the interim as its own trailing partial line (never merged)', () => {
    const lines = toLiveLines({
      committed: [{ text: 'Confirmed part.', source: 'Me' }],
      interim: { text: 'still forming', source: 'Me', is_partial: true },
    });
    expect(lines).toHaveLength(2);
    expect(lines[1]).toEqual({ role: 'me', label: 'You', text: 'still forming', partial: true });
  });

  it('defaults an un-attributed (no source) segment to You, never a stranger', () => {
    const lines = toLiveLines({ committed: [{ text: 'no speaker signal' }], interim: null });
    expect(lines[0].role).toBe('me');
    expect(lines[0].label).toBe('You');
  });

  it('drops empty segments', () => {
    const lines = toLiveLines({ committed: [{ text: '   ', source: 'Me' }, { text: 'real', source: 'Me' }], interim: null });
    expect(lines).toHaveLength(1);
    expect(lines[0].text).toBe('real');
  });
});

describe('reduceLive — interim stream', () => {
  it('shows the LATEST interim, replacing prior (never appends duplicates)', () => {
    let s = emptyLive;
    s = reduceLive(s, { text: 'thanks for', is_partial: true });
    s = reduceLive(s, { text: 'thanks for making', is_partial: true });
    s = reduceLive(s, { text: 'thanks for making the time', is_partial: true });
    expect(s.interim?.text).toBe('thanks for making the time');
    expect(s.committed).toHaveLength(0);
  });

  it('on finalize: commits exactly once and clears the interim', () => {
    let s = emptyLive;
    s = reduceLive(s, { text: 'thanks for making the tim', is_partial: true });
    s = reduceLive(s, { text: 'Thanks for making the time today.', is_partial: false, sequence_id: 1 });
    expect(s.interim).toBeNull();
    expect(s.committed).toHaveLength(1);
    expect(s.committed[0].text).toBe('Thanks for making the time today.');
    // a duplicate final (same sequence_id) REPLACES, does not append
    s = reduceLive(s, { text: 'Thanks for making the time today.', is_partial: false, sequence_id: 1 });
    expect(s.committed).toHaveLength(1);
  });

  it('handles a full two-utterance stream (interim… → final → interim… → final)', () => {
    let s = emptyLive;
    for (const t of ['I', 'I have', 'I have questions']) s = reduceLive(s, { text: t, is_partial: true });
    s = reduceLive(s, { text: 'I have questions on pricing.', is_partial: false, sequence_id: 1 });
    for (const t of ['and', 'and the']) s = reduceLive(s, { text: t, is_partial: true });
    s = reduceLive(s, { text: 'And the timeline.', is_partial: false, sequence_id: 2 });
    expect(s.committed.map((c) => c.text)).toEqual(['I have questions on pricing.', 'And the timeline.']);
    expect(s.interim).toBeNull();
  });
});

describe('deriveLive — snapshot over the segment array', () => {
  it('a trailing in-progress segment is the interim; the rest is committed', () => {
    const st = deriveLive([
      { text: 'first', is_partial: false, sequence_id: 1 },
      { text: 'second so far', is_partial: true, sequence_id: 2 },
    ]);
    expect(st.committed.map((c) => c.text)).toEqual(['first']);
    expect(st.interim?.text).toBe('second so far');
  });

  it('all finals → no interim', () => {
    const st = deriveLive([
      { text: 'a', is_partial: false },
      { text: 'b', is_partial: false },
    ]);
    expect(st.interim).toBeNull();
    expect(st.committed).toHaveLength(2);
  });
});

describe('interim is DISPLAY-ONLY (never saved/uploaded)', () => {
  it('formatTranscript excludes interim (is_partial) segments from the export', () => {
    const out = formatTranscript(
      [
        { text: 'This is final.', is_partial: false, audio_start_time: 1, sequence_id: 1, source: 'Me' },
        { text: 'this interim should not be saved', is_partial: true, audio_start_time: 2, sequence_id: 2, source: 'Me' },
      ],
      { header: false },
    );
    expect(out).toContain('This is final.');
    expect(out).not.toContain('this interim should not be saved');
  });
});

describe('recognitionActivity — fallback live signal', () => {
  it('maps recording/paused/speaking/interim to the right state', () => {
    expect(recognitionActivity({ recording: true, paused: false, hasInterim: false, speaking: true })).toBe('recognizing');
    expect(recognitionActivity({ recording: true, paused: false, hasInterim: true, speaking: false })).toBe('recognizing');
    expect(recognitionActivity({ recording: true, paused: false, hasInterim: false, speaking: false })).toBe('listening');
    expect(recognitionActivity({ recording: true, paused: true, hasInterim: false, speaking: false })).toBe('paused');
    expect(recognitionActivity({ recording: false, paused: false, hasInterim: false, speaking: false })).toBe('idle');
  });
});

describe('rmsFromTimeDomain (VU level)', () => {
  it('silence → 0, signal → > 0', () => {
    expect(rmsFromTimeDomain(new Uint8Array(64).fill(128))).toBe(0);
    const loud = new Uint8Array(64);
    for (let i = 0; i < loud.length; i++) loud[i] = i % 2 === 0 ? 255 : 0;
    expect(rmsFromTimeDomain(loud)).toBeGreaterThan(0);
  });
});
