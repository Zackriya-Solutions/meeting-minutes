import { describe, expect, test } from 'bun:test';
import { searchMatchLabel } from '../../src/lib/search-match';

describe('searchMatchLabel', () => {
  test('labels a match found in notes', () => {
    expect(searchMatchLabel('notes')).toBe('Match in notes:');
  });

  test('labels transcript matches and results without a source', () => {
    expect(searchMatchLabel('transcript')).toBe('Match:');
    expect(searchMatchLabel(undefined)).toBe('Match:');
  });
});
