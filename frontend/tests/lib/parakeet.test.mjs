import { describe, expect, test } from "bun:test";
import {
  getTranscriptionLanguageCapability,
  supportsTranscriptionLanguage,
} from '../../src/lib/parakeet';

describe('Parakeet model metadata', () => {
  test('keeps English TDT and selectable-language providers distinct', () => {
    expect(
      getTranscriptionLanguageCapability('parakeet', 'parakeet-tdt-0.6b-v3-int8')
        .analyticsLanguage
    ).toBe('en');
    expect(getTranscriptionLanguageCapability('localWhisper', 'large-v3'))
      .toMatchObject({ allowsLanguageSelection: true });
  });

  test('allows supported Qwen3-ASR language hints without translation mode', () => {
    const capability = getTranscriptionLanguageCapability(
      'qwen3Asr',
      'qwen3-asr-0.6b-int8'
    );
    expect(capability.allowsLanguageSelection).toBe(true);
    expect(supportsTranscriptionLanguage(capability, 'zh')).toBe(true);
    expect(supportsTranscriptionLanguage(capability, 'yue')).toBe(true);
    expect(supportsTranscriptionLanguage(capability, 'auto')).toBe(true);
    expect(supportsTranscriptionLanguage(capability, 'auto-translate')).toBe(false);
    expect(supportsTranscriptionLanguage(capability, 'uk')).toBe(false);
    expect(capability.description).toContain('Choose Chinese');
  });
});
