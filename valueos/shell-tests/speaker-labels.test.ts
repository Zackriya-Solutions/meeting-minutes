import { describe, it, expect } from 'vitest';
import { sourceToLabel, labelTranscript } from '@/valueos/capture/speakerLabels';

// VALUEOS WS3: label-mapping logic (transcription output mocked as plain segment objects).
// The stack today emits no per-segment source (achieved level: c), so the CRITICAL property
// is that labelTranscript is a no-op vs. a plain join until a source is present — while being
// ready to produce "Me:"/"Other:"/"Speaker N:" the moment upstream populates `source`.

describe('sourceToLabel', () => {
  it('maps mic-side sources to "Me"', () => {
    for (const s of ['Microphone', 'mic', 'ME', ' microphone ']) expect(sourceToLabel(s)).toBe('Me');
  });
  it('maps received/system sources to "Other"', () => {
    for (const s of ['System', 'system', 'received', 'Other', 'speaker']) expect(sourceToLabel(s)).toBe('Other');
  });
  it('maps enumerated speakers to "Speaker N"', () => {
    expect(sourceToLabel('Speaker 2')).toBe('Speaker 2');
    expect(sourceToLabel('speaker_3')).toBe('Speaker 3');
    expect(sourceToLabel('SPEAKER-4')).toBe('Speaker 4');
  });
  it('returns null for the current constant / unknown / empty sources', () => {
    for (const s of ['Audio', 'audio', 'weird', '', null, undefined]) expect(sourceToLabel(s)).toBeNull();
  });
});

describe('labelTranscript', () => {
  it('is a plain newline join when segments carry NO source (level c — no regression)', () => {
    const segs = [{ text: 'Hello Ada.' }, { text: 'Discussed pricing.' }, { text: '   ' }];
    expect(labelTranscript(segs)).toBe('Hello Ada.\nDiscussed pricing.'); // blanks dropped, no prefixes
  });

  it('is a plain join even when source is the hardcoded "Audio"', () => {
    const segs = [
      { text: 'line one', source: 'Audio' },
      { text: 'line two', source: 'Audio' },
    ];
    expect(labelTranscript(segs)).toBe('line one\nline two');
  });

  it('prefixes lines with speaker labels once a real source is present (ready for level a)', () => {
    const segs = [
      { text: 'Are you seeing the dashboard?', source: 'Microphone' },
      { text: 'Yes, it just loaded.', source: 'System' },
      { text: 'Great — walk me through it.', source: 'mic' },
    ];
    expect(labelTranscript(segs)).toBe(
      'Me: Are you seeing the dashboard?\nOther: Yes, it just loaded.\nMe: Great — walk me through it.',
    );
  });
});
