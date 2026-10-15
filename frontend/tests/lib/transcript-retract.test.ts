import { describe, expect, test } from 'bun:test';
import { mergeHistory, withoutRetracted } from '../../src/lib/transcript-retract';

describe('transcript retraction', () => {
  test('removes only the retracted row', () => {
    const rows = [{ sequence_id: 1, text: 'a' }, { sequence_id: 2, text: 'b' }, { sequence_id: 3, text: 'c' }];
    expect(withoutRetracted(rows, 2).map((r) => r.text)).toEqual(['a', 'c']);
  });

  test('returns the same array when nothing matches, so React skips a render', () => {
    const rows = [{ sequence_id: 1 }];
    expect(withoutRetracted(rows, 9)).toBe(rows);
  });

  test('reload sync keeps live rows that arrived during the fetch and drops retracted ones', () => {
    const history = [{ sequence_id: 1, chunk_start_time: 1, text: 'a' }, { sequence_id: 2, chunk_start_time: 2, text: 'echo' }];
    const current = [{ sequence_id: 3, chunk_start_time: 0.5, text: 'arrived during fetch' }];
    const merged = mergeHistory(history, current, new Set([2]));
    // Ordered by audio time, like the live view.
    expect(merged.map((r) => r.text)).toEqual(['arrived during fetch', 'a']);
  });
});
