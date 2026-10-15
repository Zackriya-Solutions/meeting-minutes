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

  test('numbered diarized labels keep their channel colour', () => {
    expect(speakerLabelClass('Speaker-Out 2')).toBe(speakerLabelClass(SPEAKER_OUT_LABEL));
    expect(speakerLabelClass('Mic-In 1')).toBe(speakerLabelClass(MIC_IN_LABEL));
    // A diarized mono stream has no channel.
    expect(speakerLabelClass('Speaker 1')).toBe('text-gray-500');
    expect(formatTranscriptLine('Hello.', 'Speaker-Out 2')).toBe('Speaker-Out 2: Hello.');
  });
});
