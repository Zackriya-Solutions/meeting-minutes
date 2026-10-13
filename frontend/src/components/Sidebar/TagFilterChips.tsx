'use client';

import { Settings2 } from 'lucide-react';
import { tagsForChips, type Tag } from '@/lib/organization';
import { TagChip } from './TagChip';

interface TagFilterChipsProps {
  tags: Tag[];
  selectedIds: string[];
  onToggle: (tagId: string) => void;
  onManage: () => void;
}

export function TagFilterChips({ tags, selectedIds, onToggle, onManage }: TagFilterChipsProps) {
  const chips = tagsForChips(tags, selectedIds);
  if (chips.length === 0) return null;
  return (
    <div className="flex flex-wrap gap-1 px-3 pt-2">
      {chips.map(tag => (
        <TagChip key={tag.id} tag={tag} selected={selectedIds.includes(tag.id)} onToggle={onToggle} />
      ))}
      <button
        type="button"
        onClick={onManage}
        aria-label="Manage tags"
        className="rounded-full p-1 text-gray-400 hover:bg-gray-100 hover:text-gray-600"
      >
        <Settings2 className="h-3 w-3" />
      </button>
    </div>
  );
}
