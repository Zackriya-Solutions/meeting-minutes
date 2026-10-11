import type { Person } from '@/types';

/** People whose name contains the draft, names that start with it first; at most `limit`. */
export function matchPeople(people: Person[], draft: string, limit = 5): Person[] {
  const query = draft.trim().replace(/\s+/g, ' ').toLowerCase();
  if (!query) return [];
  const prefix: Person[] = [];
  const inside: Person[] = [];
  for (const person of people) {
    const name = person.name.toLowerCase();
    if (name.startsWith(query)) prefix.push(person);
    else if (name.includes(query)) inside.push(person);
  }
  return [...prefix, ...inside].slice(0, limit);
}

export function formatMeetingCount(count: number): string {
  return `${count} meeting${count === 1 ? '' : 's'}`;
}

/** A person's last meeting as a short local date; 'never' without one, the raw text if unreadable. */
export function formatLastSeen(lastSeen: string | null): string {
  if (!lastSeen) return 'never';
  const date = new Date(lastSeen);
  if (Number.isNaN(date.getTime())) return lastSeen;
  return date.toLocaleDateString(undefined, { year: 'numeric', month: 'short', day: 'numeric' });
}
