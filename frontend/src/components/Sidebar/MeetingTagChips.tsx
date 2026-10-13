'use client';

import type { Tag } from '@/lib/organization';
import { TagChip } from './TagChip';

interface MeetingTagChipsProps {
  tagIds: string[];
  tags: Tag[];
  selectedIds: string[];
  onToggle: (tagId: string) => void;
}

export function MeetingTagChips({ tagIds, tags, selectedIds, onToggle }: MeetingTagChipsProps) {
  const rowTags = tags.filter(t => tagIds.includes(t.id));
  if (rowTags.length === 0) return null;
  return (
    <div className="ml-8 mt-1 flex flex-wrap gap-1 font-normal">
      {rowTags.map(tag => (
        <TagChip key={tag.id} tag={tag} selected={selectedIds.includes(tag.id)} onToggle={onToggle} compact />
      ))}
    </div>
  );
}
