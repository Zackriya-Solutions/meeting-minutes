// Gemini Live (cloud) transcription: Tauri command wrappers. Fork addition.
import { invoke } from '@tauri-apps/api/core';

export const GEMINI_LIVE_PROVIDER = 'geminiLive';
export const GEMINI_LIVE_MODEL = 'gemini-3.5-transcribe-live';

export type GeminiLanguageMode = 'roman' | 'devanagari' | 'auto';
export type GeminiTranscriptionMode = 'VERBATIM' | 'SMART';
export type GeminiFallbackProvider = 'parakeet' | 'localWhisper';

export interface GeminiLiveSettings {
  languageMode: GeminiLanguageMode;
  customVocabulary: string[];
  transcriptionMode: GeminiTranscriptionMode;
  fallbackProvider: GeminiFallbackProvider;
}

export interface GeminiLiveSettingsView extends GeminiLiveSettings {
  hasApiKey: boolean;
  apiKeyHint: string | null;
  hasSummaryApiKey: boolean;
}

export interface GeminiLiveStatus {
  kind: 'fallback' | 'resumed' | 'disabled';
  message: string;
}

export const MAX_VOCABULARY_TERMS = 1000;

export const getGeminiLiveSettings = () =>
  invoke<GeminiLiveSettingsView>('gemini_live_get_settings');

/** apiKey: new key, '' to remove the saved key, null to keep it. */
export const saveGeminiLiveSettings = (settings: GeminiLiveSettings, apiKey: string | null) =>
  invoke<GeminiLiveSettings>('gemini_live_save_settings', { settings, apiKey });

export const testGeminiLiveConnection = (apiKey: string | null) =>
  invoke<string>('gemini_live_test_connection', { apiKey });

/** Make Gemini Live the active transcription engine. */
export const activateGeminiLive = () =>
  invoke('api_save_transcript_config', {
    provider: GEMINI_LIVE_PROVIDER,
    model: GEMINI_LIVE_MODEL,
    apiKey: null,
  });

/**
 * Gemini Live needs a downloaded local model for fallback, so recording readiness checks
 * use the fallback engine. Returns the local provider to check.
 */
export async function getGeminiLiveFallbackProvider(): Promise<GeminiFallbackProvider> {
  try {
    return (await getGeminiLiveSettings()).fallbackProvider;
  } catch (error) {
    console.error('Failed to load Gemini Live settings:', error);
    return 'parakeet';
  }
}
