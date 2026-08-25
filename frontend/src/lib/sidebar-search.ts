export interface SearchableSidebarItem {
  id: string;
  title: string;
  type: 'folder' | 'file';
  children?: SearchableSidebarItem[];
  tags?: string[];
}

export function isFolderExpanded(
  folderId: string,
  expandedFolders: Set<string>,
  searchQuery: string
): boolean {
  return searchQuery.trim() !== '' || expandedFolders.has(folderId);
}

export function filterSidebarItems<T extends SearchableSidebarItem>(
  items: T[],
  searchQuery: string,
  matchedMeetingIds: Set<string>
): T[] {
  if (!searchQuery.trim()) return items;
  const query = searchQuery.toLowerCase();

  const filterItem = (item: T, isRoot = false): T | undefined => {
    if (item.type === 'folder') {
      if (!isRoot && item.title.toLowerCase().includes(query)) return item;
      const children = (item.children ?? [])
        .map(child => filterItem(child as T))
        .filter((child): child is T => child !== undefined);
      return isRoot || children.length > 0 ? { ...item, children } : undefined;
    }
    const matches = matchedMeetingIds.has(item.id) ||
      item.title.toLowerCase().includes(query) ||
      (item.tags ?? []).some(tag => tag.toLowerCase().includes(query));
    return matches ? item : undefined;
  };

  return items.map(item => filterItem(item, true)).filter((item): item is T => item !== undefined);
}
