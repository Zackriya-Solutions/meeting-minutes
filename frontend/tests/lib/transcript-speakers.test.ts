import { describe, expect, test } from 'bun:test';
import {
  MIC_IN_LABEL,
  SPEAKER_OUT_LABEL,
  formatTranscriptLine,
  speakerLabelClass,
} from '../../src/lib/transcript-speakers';

describe('transcript speaker labels', () => {
  test('prefixes labelled lines and leaves unlabelled ones alone', () => {
    expect(formatTranscriptLine('Hello.', MIC_IN_LABEL)).toBe('Mic-In: Hello.');
    expect(formatTranscriptLine('Hi there.', SPEAKER_OUT_LABEL)).toBe('Speaker-Out: Hi there.');
    expect(formatTranscriptLine('Mixed.', undefined)).toBe('Mixed.');
    expect(formatTranscriptLine('Mixed.', null)).toBe('Mixed.');
  });

  test('styles each known label distinctly and unknown labels neutrally', () => {
    const mic = speakerLabelClass(MIC_IN_LABEL);
    const speaker = speakerLabelClass(SPEAKER_OUT_LABEL);
    const unknown = speakerLabelClass('Room-Mic');
    expect(new Set([mic, speaker, unknown]).size).toBe(3);
    expect(unknown).toBe('text-gray-500');
  });
});
