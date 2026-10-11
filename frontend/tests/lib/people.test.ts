import { describe, expect, test } from 'bun:test';
import { formatLastSeen, formatMeetingCount, matchPeople } from '../../src/lib/people';
import type { Person } from '../../src/types';

const person = (id: string, name: string): Person => ({ id, name, meeting_count: 1, last_seen: null });
const people = [
  person('p1', 'Ana'), person('p2', 'Joanna'), person('p3', 'Noah'), person('p4', 'Annie'), person('p5', 'Hannah'),
];

describe('matchPeople', () => {
  test('prefix matches come before substring matches', () => {
    expect(matchPeople(people, 'an').map((p) => p.name)).toEqual(['Ana', 'Annie', 'Joanna', 'Hannah']);
  });

  test('matching ignores case and surrounding spaces', () => {
    expect(matchPeople(people, '  NOA ').map((p) => p.name)).toEqual(['Noah']);
  });

  test('results are capped at the limit', () => {
    expect(matchPeople(people, 'a', 2)).toHaveLength(2);
    expect(matchPeople(people, 'a')).toHaveLength(5);
  });

  test('an empty draft offers nothing', () => {
    expect(matchPeople(people, '')).toEqual([]);
    expect(matchPeople(people, '   ')).toEqual([]);
  });
});

describe('formatLastSeen', () => {
  test('never seen and unreadable dates', () => {
    expect(formatLastSeen(null)).toBe('never');
    expect(formatLastSeen('not a date')).toBe('not a date');
  });

  test('dates use the local short format', () => {
    const iso = '2026-10-01T10:00:00+00:00';
    expect(formatLastSeen(iso)).toBe(new Date(iso).toLocaleDateString(undefined, { year: 'numeric', month: 'short', day: 'numeric' }));
  });
});

describe('formatMeetingCount', () => {
  test('is singular for one meeting and plural otherwise', () => {
    expect(formatMeetingCount(1)).toBe('1 meeting');
    expect(formatMeetingCount(0)).toBe('0 meetings');
    expect(formatMeetingCount(3)).toBe('3 meetings');
  });
});
