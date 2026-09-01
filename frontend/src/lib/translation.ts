import type { Transcript } from '@/types';

export interface TranslationSettings {
  enabled: boolean;
  engine: 'libretranslate' | 'ollama';
  targetLanguage: string;
  libretranslateEndpoint: string;
  ollamaEndpoint: string;
  ollamaModel: string;
}

export const DEFAULT_TARGET_LANGUAGE = 'pt-BR';
export const DEFAULT_LIBRETRANSLATE_ENDPOINT = '';
export const DEFAULT_OLLAMA_ENDPOINT = 'http://localhost:11434';
export const DEFAULT_OLLAMA_MODEL = 'aya-expanse:latest';

export interface TranslationUpdate {
  sequence_id: number;
  translation: string | null;
  target_language: string;
  status: 'pending' | 'ready' | 'error' | 'disabled';
  error?: string;
}

/** Merge a backend translation event into an existing transcript row. */
export function mergeTranslationUpdate(
  transcripts: Transcript[],
  update: TranslationUpdate,
): Transcript[] {
  const index = transcripts.findIndex(segment => segment.sequence_id === update.sequence_id);
  if (index === -1) return transcripts;

  const merged = [...transcripts];
  merged[index] = {
    ...merged[index],
    translation: update.translation ?? undefined,
    translation_target_language: update.target_language,
    translation_status: update.status,
    translation_error: update.error,
  };
  return merged;
}
