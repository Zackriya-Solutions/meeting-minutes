import { useCallback, useEffect, useState } from 'react';
import { normaliseLanguageCode } from '@/lib/summary-languages';
import {
  SUMMARY_LANGUAGE_DEFAULT_KEY,
  SUMMARY_LANGUAGE_RECENTS_KEY,
  readPinnedSummaryLanguageDefault,
  writePinnedSummaryLanguageDefault,
} from '@/lib/summary-language-preferences';

const MRU_KEY = SUMMARY_LANGUAGE_RECENTS_KEY;
const PINNED_KEY = SUMMARY_LANGUAGE_DEFAULT_KEY;
const MAX_RECENTS = 5;

function readPinnedFromStorage(): string | null {
  return readPinnedSummaryLanguageDefault();
}

function writePinnedToStorage(value: string | null): void {
  writePinnedSummaryLanguageDefault(value);
}

function readFromStorage(): string[] {
  if (typeof window === 'undefined') return [];
  try {
    const raw = window.localStorage.getItem(MRU_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    const normalised: string[] = [];
    for (const item of parsed) {
      if (typeof item !== 'string') continue;
      const code = normaliseLanguageCode(item);
      if (code && !normalised.includes(code)) normalised.push(code);
      if (normalised.length >= MAX_RECENTS) break;
    }
    return normalised;
  } catch {
    return [];
  }
}

function writeToStorage(values: string[]): void {
  if (typeof window === 'undefined') return;
  try {
    window.localStorage.setItem(MRU_KEY, JSON.stringify(values));
  } catch {
    // Quota exceeded / incognito — cosmetic list only, silent.
  }
}

/**
 * MRU list of recently used summary languages (max 5, localStorage).
 * Shared by SummaryLanguageSettings (chips) and LanguagePickerPopover (recents).
 *
 * addRecent: push to front, dedupe, trim to MAX_RECENTS, persist.
 */
export function useRecentLanguages() {
  const [recents, setRecents] = useState<string[]>(() => readFromStorage());
  const [pinned, setPinnedState] = useState<string | null>(() => readPinnedFromStorage());

  useEffect(() => {
    const onStorage = (e: StorageEvent) => {
      if (e.key === MRU_KEY) setRecents(readFromStorage());
      if (e.key === PINNED_KEY) setPinnedState(readPinnedFromStorage());
    };
    window.addEventListener('storage', onStorage);
    return () => window.removeEventListener('storage', onStorage);
  }, []);

  const addRecent = useCallback((code: string) => {
    const normalised = normaliseLanguageCode(code);
    if (!normalised) return;
    setRecents((prev) => {
      const deduped = [normalised, ...prev.filter((c) => c !== normalised)].slice(0, MAX_RECENTS);
      writeToStorage(deduped);
      return deduped;
    });
  }, []);

  const removeRecent = useCallback((code: string) => {
    const normalised = normaliseLanguageCode(code) ?? code;
    setRecents((prev) => {
      const updated = prev.filter((c) => c !== normalised);
      writeToStorage(updated);
      return updated;
    });
    setPinnedState((prev) => {
      if (prev !== normalised) return prev;
      writePinnedToStorage(null);
      return null;
    });
  }, []);

  const setPinned = useCallback((code: string | null) => {
    const normalised = code ? normaliseLanguageCode(code) : null;
    setPinnedState(normalised);
    writePinnedToStorage(normalised);
  }, []);

  return { recents, pinned, addRecent, removeRecent, setPinned };
}
