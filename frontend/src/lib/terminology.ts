import { invoke } from '@tauri-apps/api/core';

/** A correction: occurrences of `wrong` are replaced with `correct`. */
export interface TerminologyEntry {
  wrong: string;
  correct: string;
  /** When false, `wrong` matches regardless of letter case. */
  matchCase: boolean;
}

export const getTerminology = (): Promise<TerminologyEntry[]> =>
  invoke<TerminologyEntry[]>('api_get_terminology');

export const saveTerminology = (entries: TerminologyEntry[]): Promise<TerminologyEntry[]> =>
  invoke<TerminologyEntry[]>('api_save_terminology', { entries });

export const addTerminologyEntry = (entry: TerminologyEntry): Promise<TerminologyEntry[]> =>
  invoke<TerminologyEntry[]>('api_add_terminology_entry', { entry });

const WORD_CHAR = new RegExp('[\\p{L}\\p{N}_]', 'u');

const escapeRegExp = (value: string): string => value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');

/**
 * Builds a whole-word matcher for `phrase`, mirroring the backend rules: word boundaries
 * are only enforced where the phrase starts or ends with a word character.
 */
export function buildPhraseRegExp(phrase: string, matchCase: boolean): RegExp {
  const startsWithWord = WORD_CHAR.test(phrase.charAt(0));
  const endsWithWord = WORD_CHAR.test(phrase.charAt(phrase.length - 1));
  const source =
    (startsWithWord ? '(?<![\\p{L}\\p{N}_])' : '') +
    escapeRegExp(phrase) +
    (endsWithWord ? '(?![\\p{L}\\p{N}_])' : '');
  return new RegExp(source, matchCase ? 'gu' : 'giu');
}

/**
 * Parses an imported terminology file. Accepts Meetily's own `{ entries: [...] }` format and
 * the `{ corrections: [{ correct, variants: [...] }] }` format used by template tooling.
 */
export function parseTerminologyImport(json: string): TerminologyEntry[] {
  const data = JSON.parse(json);

  if (Array.isArray(data?.entries)) {
    return data.entries.map((e: Partial<TerminologyEntry>) => ({
      wrong: String(e.wrong ?? ''),
      correct: String(e.correct ?? ''),
      matchCase: Boolean(e.matchCase),
    }));
  }

  if (Array.isArray(data?.corrections)) {
    return data.corrections.flatMap((c: { correct?: string; variants?: string[] }) =>
      (c.variants ?? []).map((wrong) => ({
        wrong: String(wrong),
        correct: String(c.correct ?? ''),
        matchCase: false,
      })),
    );
  }

  throw new Error('Unrecognized file. Expected an "entries" or "corrections" list.');
}

export const serializeTerminology = (entries: TerminologyEntry[]): string =>
  JSON.stringify({ entries }, null, 2);
