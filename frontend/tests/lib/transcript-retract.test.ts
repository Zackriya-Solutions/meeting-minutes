import { describe, expect, test } from 'bun:test';
import { withoutRetracted } from '../../src/lib/transcript-retract';

describe('transcript retraction', () => {
  test('removes only the retracted row', () => {
    const rows = [{ sequence_id: 1, text: 'a' }, { sequence_id: 2, text: 'b' }, { sequence_id: 3, text: 'c' }];
    expect(withoutRetracted(rows, 2).map((r) => r.text)).toEqual(['a', 'c']);
  });

  test('returns the same array when nothing matches, so React skips a render', () => {
    const rows = [{ sequence_id: 1 }];
    expect(withoutRetracted(rows, 9)).toBe(rows);
  });
});
