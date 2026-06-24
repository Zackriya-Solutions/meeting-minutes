// Helpers for the meeting-log features: real-time translation (to Thai or
// English, with per-segment+target cache so switching never re-translates) and
// clipboard copy.

import { invoke } from '@tauri-apps/api/core';

/** Transcript language view (spec: Original / Thai / English / Bilingual). */
export type LangView = 'original' | 'thai' | 'english' | 'bilingual';

export const LANG_LABEL: Record<LangView, string> = {
  original: 'Original',
  thai: 'ไทย',
  english: 'English',
  bilingual: 'Bilingual',
};

export const LANG_ORDER: LangView[] = ['original', 'thai', 'english', 'bilingual'];

/** Translation target for a view, or null when nothing should be translated. */
export function targetForView(view: LangView): 'th' | 'en' | null {
  switch (view) {
    case 'thai':
      return 'th';
    case 'english':
      return 'en';
    case 'bilingual':
      return 'th'; // bilingual shows original + Thai translation
    default:
      return null;
  }
}

const THAI_RE = /[฀-๿]/;
const LATIN_RE = /[A-Za-z]/;

export function hasThai(text: string): boolean {
  return THAI_RE.test(text);
}

/** True when `text` is already entirely in the target language. */
export function alreadyTarget(text: string, target: 'th' | 'en'): boolean {
  return target === 'th' ? !LATIN_RE.test(text) : !THAI_RE.test(text);
}

// Cache keyed by `${target}:${rawText}`.
const cache = new Map<string, string>();

/**
 * Translate one segment into `target`, pinning technical terms (in the sidecar).
 * Text already in the target language is returned unchanged. Cached per target.
 */
export async function translateSegment(text: string, target: 'th' | 'en'): Promise<string> {
  const key = text.trim();
  if (!key) return '';
  if (alreadyTarget(key, target)) return key;
  const ck = `${target}:${key}`;
  const cached = cache.get(ck);
  if (cached !== undefined) return cached;
  try {
    const out = await invoke<string>('meeting_log_translate', { text: key, target });
    const result = (out || '').trim() || key;
    cache.set(ck, result);
    return result;
  } catch (e) {
    console.error('translateSegment failed:', e);
    return key; // fail open
  }
}

/** Copy plain text to the clipboard; returns success. */
export async function copyText(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    try {
      const ta = document.createElement('textarea');
      ta.value = text;
      ta.style.position = 'fixed';
      ta.style.opacity = '0';
      document.body.appendChild(ta);
      ta.select();
      document.execCommand('copy');
      document.body.removeChild(ta);
      return true;
    } catch {
      return false;
    }
  }
}
