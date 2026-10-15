export type SearchMatchSource = 'transcript' | 'notes';

/** Label shown before a search snippet in the sidebar. */
export function searchMatchLabel(source?: SearchMatchSource): string {
  return source === 'notes' ? 'Match in notes:' : 'Match:';
}
