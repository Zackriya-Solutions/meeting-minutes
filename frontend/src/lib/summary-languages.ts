import { getLanguageDisplayName } from '@/constants/languages';

export interface LanguageOption {
  code: string;
  label: string;
}

/**
 * Language options offered in the summary language pickers.
 * Codes must stay in sync with `language_name_from_code` in
 * `frontend/src-tauri/src/summary/processor.rs`.
 */
const SUMMARY_LANGUAGE_CODES = [
  'en', 'zh', 'zh-tw', 'de', 'es', 'ru', 'ko', 'fr', 'ja', 'pt',
  'it', 'nl', 'pl', 'ar', 'hi', 'ta', 'tr', 'vi', 'th', 'id',
  'sv', 'cs', 'da', 'fi', 'el', 'he', 'hu', 'no', 'ro', 'uk',
] as const;

export const LANGUAGE_OPTIONS: LanguageOption[] = SUMMARY_LANGUAGE_CODES.map((code) => ({
  code,
  label: getLanguageDisplayName(code),
}));

export const AUTO_VALUE = '__auto__' as const;

const SUPPORTED_CODES: ReadonlySet<string> = new Set(LANGUAGE_OPTIONS.map((o) => o.code));

/**
 * Normalises a raw locale string (from transcription or storage) into a code we
 * can translate into. Handles BCP-47 regional tags: `pt-BR` -> `pt`, `en_GB` -> `en`.
 * Returns null for unsupported languages so callers can fall back to English
 * rather than sending a code Rust will silently drop.
 */
export function normaliseLanguageCode(raw: string | null | undefined): string | null {
  if (!raw) return null;
  const lower = raw.toLowerCase().replace(/_/g, '-');
  if (SUPPORTED_CODES.has(lower)) return lower;
  const base = lower.split('-')[0];
  if (SUPPORTED_CODES.has(base)) return base;
  return null;
}

export function labelForCode(code: string): string {
  return LANGUAGE_OPTIONS.find((l) => l.code === code)?.label ?? code;
}
