import { describe, expect, test } from "bun:test";
import {
  PARAKEET_CTC_ZH_CN_MODEL,
  getModelDisplayName,
  getModelDownloadProgress,
  getParakeetModelSizeMb,
  getTranscriptionLanguageCapability,
} from '../../src/lib/parakeet';

describe('Parakeet model metadata', () => {
  test('reads the Rust Downloading struct variant', () => {
    expect(getModelDownloadProgress({ Downloading: { progress: 42 } })).toBe(42);
    expect(getModelDownloadProgress('Available')).toBeNull();
  });

  test('describes the Mandarin CoreML model', () => {
    expect(getModelDisplayName(PARAKEET_CTC_ZH_CN_MODEL)).toBe('Mandarin CoreML');
    expect(getParakeetModelSizeMb(PARAKEET_CTC_ZH_CN_MODEL)).toBe(582);

    const capability = getTranscriptionLanguageCapability(
      'parakeet',
      PARAKEET_CTC_ZH_CN_MODEL
    );
    expect(capability.allowsLanguageSelection).toBe(false);
    expect(capability.displayName).toBe('Mandarin Chinese + English');
    expect(capability.analyticsLanguage).toBe('zh-en');
    expect(capability.description).toContain('Mandarin Chinese');
    expect(capability.description).toContain('Translation is not available');
  });

  test('keeps English TDT and selectable-language providers distinct', () => {
    expect(
      getTranscriptionLanguageCapability('parakeet', 'parakeet-tdt-0.6b-v3-int8')
        .analyticsLanguage
    ).toBe('en');
    expect(getTranscriptionLanguageCapability('localWhisper', 'large-v3'))
      .toMatchObject({ allowsLanguageSelection: true });
  });
});
