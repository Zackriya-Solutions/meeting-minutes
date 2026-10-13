'use client';

import type { Tag } from '@/lib/organization';

interface TagChipProps {
  tag: Tag;
  selected: boolean;
  onToggle: (tagId: string) => void;
  /** Smaller chip for meeting rows. */
  compact?: boolean;
}

/** A tag that toggles its sidebar filter when clicked. */
export function TagChip({ tag, selected, onToggle, compact = false }: TagChipProps) {
  return (
    <button
      type="button"
      aria-pressed={selected}
      title={selected ? `Stop filtering by #${tag.name}` : `Filter by #${tag.name}`}
      onClick={e => {
        e.stopPropagation();
        onToggle(tag.id);
      }}
      className={`max-w-full truncate rounded-full border ${
        compact ? 'px-1.5 text-[11px] leading-4' : 'px-2 py-0.5 text-xs'
      } ${
        selected
          ? 'border-blue-300 bg-blue-100 text-blue-800'
          : 'border-gray-200 bg-white text-gray-600 hover:bg-gray-100'
      }`}
    >
      #{tag.name}
    </button>
  );
}
