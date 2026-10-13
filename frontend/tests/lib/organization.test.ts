import { describe, expect, test } from 'bun:test';
import {
  buildSidebarSections,
  daysLeftLabel,
  meetingRowTags,
  tagsForChips,
  UNFILED_ID,
  type Folder,
  type MeetingListItem,
  type Tag,
} from '../../src/lib/organization';

const folder = (id: string, name: string): Folder => ({
  id, name, parent_id: null, sort_order: 0, created_at: '', updated_at: '',
});
const meeting = (id: string, title: string, folder_id: string | null, tag_ids: string[] = []): MeetingListItem => ({
  id, title, created_at: '', folder_id, tag_ids,
});
const noFilter = { selectedTagIds: [], searchQuery: '', searchResultIds: [] };

describe('buildSidebarSections', () => {
  const folders = [folder('f-work', 'work'), folder('f-arch', 'Archive')];
  const meetings = [
    meeting('m1', 'Standup', null, ['t-todo']),
    meeting('m2', 'Sprint review', 'f-work', ['t-todo', 't-client']),
    meeting('m3', 'Old call', 'f-gone'),
  ];

  test('puts Unfiled first, folders A-Z, and orphaned folder ids in Unfiled', () => {
    const sections = buildSidebarSections(meetings, folders, noFilter);
    expect(sections.map(s => s.id)).toEqual([UNFILED_ID, 'f-arch', 'f-work']);
    expect(sections[0].meetings.map(m => m.id)).toEqual(['m1', 'm3']);
    expect(sections[1].meetings).toEqual([]);
  });

  test('applies tags with AND and hides empty sections while filtering', () => {
    const sections = buildSidebarSections(meetings, folders, {
      ...noFilter, selectedTagIds: ['t-todo', 't-client'],
    });
    expect(sections.map(s => s.id)).toEqual(['f-work']);
    expect(sections[0].meetings.map(m => m.id)).toEqual(['m2']);
  });

  test('a title match without the selected tag stays hidden', () => {
    const sections = buildSidebarSections(meetings, folders, {
      selectedTagIds: ['t-client'], searchQuery: 'stand', searchResultIds: [],
    });
    expect(sections).toEqual([]);
  });

  test('search matches transcript result ids or titles', () => {
    const sections = buildSidebarSections(meetings, folders, {
      selectedTagIds: [], searchQuery: 'OLD', searchResultIds: ['m2'],
    });
    expect(sections.flatMap(s => s.meetings.map(m => m.id))).toEqual(['m3', 'm2']);
  });
});

describe('tagsForChips', () => {
  test('shows tags in use plus selected ones', () => {
    const tags: Tag[] = [
      { id: 'a', name: 'a', meeting_count: 2 },
      { id: 'b', name: 'b', meeting_count: 0 },
      { id: 'c', name: 'c', meeting_count: 0 },
    ];
    expect(tagsForChips(tags, ['c']).map(t => t.id)).toEqual(['a', 'c']);
  });
});

describe('meetingRowTags', () => {
  const tags: Tag[] = [
    { id: 'z', name: 'zeta', meeting_count: 1 },
    { id: 'a', name: 'Alpha', meeting_count: 1 },
    { id: 'm', name: 'mid', meeting_count: 1 },
    { id: 'b', name: 'beta', meeting_count: 1 },
  ];

  test('lists a meeting\'s tags A-Z and skips unknown ids', () => {
    const row = meetingRowTags(['z', 'gone', 'a'], tags, 3);
    expect(row.shown.map(t => t.name)).toEqual(['Alpha', 'zeta']);
    expect(row.hidden).toEqual([]);
  });

  test('keeps the overflow separate', () => {
    const row = meetingRowTags(['z', 'a', 'm', 'b'], tags, 3);
    expect(row.shown.map(t => t.name)).toEqual(['Alpha', 'beta', 'mid']);
    expect(row.hidden.map(t => t.name)).toEqual(['zeta']);
  });
});

describe('daysLeftLabel', () => {
  test('formats retention countdowns', () => {
    expect(daysLeftLabel(null)).toBe('Kept until you empty Trash');
    expect(daysLeftLabel(0)).toBe('Deletes today');
    expect(daysLeftLabel(1)).toBe('Deletes in 1 day');
    expect(daysLeftLabel(12)).toBe('Deletes in 12 days');
  });
});
