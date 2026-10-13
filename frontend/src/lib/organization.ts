export interface Folder {
  id: string;
  name: string;
  parent_id: string | null;
  sort_order: number;
  created_at: string;
  updated_at: string;
}

/** A live tag with how many non-trashed meetings carry it. */
export interface Tag {
  id: string;
  name: string;
  meeting_count: number;
}

/** Tag as returned by create/rename. */
export interface TagRecord {
  id: string;
  name: string;
  created_at: string;
  updated_at: string;
}

export interface MeetingListItem {
  id: string;
  title: string;
  created_at: string;
  folder_id: string | null;
  tag_ids: string[];
}

export interface TrashItem {
  id: string;
  title: string;
  deleted_at: string;
  days_left: number | null;
}

export interface PurgeOutcome {
  purged: string[];
  failed: string[];
}

export interface SectionFilter {
  selectedTagIds: string[];
  searchQuery: string;
  searchResultIds: string[];
}

export interface SidebarSection {
  id: string;
  title: string;
  folder: Folder | null;
  meetings: MeetingListItem[];
}

/** Folder/tags chosen on the record screen, applied once the meeting is saved. */
export interface PendingOrganization {
  folderId: string | null;
  tagIds: string[];
}

export const UNFILED_ID = '__unfiled__';
export const EMPTY_PENDING_ORGANIZATION: PendingOrganization = { folderId: null, tagIds: [] };

/** SectionFilter with the search result ids indexed for per-meeting lookups. */
export type IndexedSectionFilter = Omit<SectionFilter, 'searchResultIds'> & {
  searchResultIds: ReadonlySet<string>;
};

export function meetingMatchesFilter(meeting: MeetingListItem, filter: IndexedSectionFilter): boolean {
  if (!filter.selectedTagIds.every(id => meeting.tag_ids.includes(id))) return false;
  const query = filter.searchQuery.trim().toLowerCase();
  if (!query) return true;
  return filter.searchResultIds.has(meeting.id) || meeting.title.toLowerCase().includes(query);
}

/**
 * Unfiled first, then folders A-Z. Meetings keep their incoming order.
 * While a tag or search filter is active, empty sections are dropped.
 */
export function buildSidebarSections(
  meetings: MeetingListItem[],
  folders: Folder[],
  filter: SectionFilter,
): SidebarSection[] {
  const filtering = filter.selectedTagIds.length > 0 || filter.searchQuery.trim() !== '';
  const known = new Set(folders.map(f => f.id));
  const indexed = { ...filter, searchResultIds: new Set(filter.searchResultIds) };

  const unfiledMeetings: MeetingListItem[] = [];
  const byFolder = new Map<string, MeetingListItem[]>();
  for (const m of meetings) {
    if (!meetingMatchesFilter(m, indexed)) continue;
    if (!m.folder_id || !known.has(m.folder_id)) {
      unfiledMeetings.push(m);
    } else {
      const bucket = byFolder.get(m.folder_id);
      if (bucket) bucket.push(m);
      else byFolder.set(m.folder_id, [m]);
    }
  }

  const unfiled: SidebarSection = { id: UNFILED_ID, title: 'Unfiled', folder: null, meetings: unfiledMeetings };
  const folderSections: SidebarSection[] = [...folders]
    .sort((a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: 'base' }))
    .map(f => ({ id: f.id, title: f.name, folder: f, meetings: byFolder.get(f.id) ?? [] }));

  const sections = [unfiled, ...folderSections];
  return filtering ? sections.filter(s => s.meetings.length > 0) : sections;
}

export function tagsForChips(tags: Tag[], selectedIds: string[]): Tag[] {
  return tags.filter(t => t.meeting_count > 0 || selectedIds.includes(t.id));
}

export function daysLeftLabel(daysLeft: number | null): string {
  if (daysLeft === null) return 'Kept until you empty Trash';
  if (daysLeft <= 0) return 'Deletes today';
  if (daysLeft === 1) return 'Deletes in 1 day';
  return `Deletes in ${daysLeft} days`;
}
