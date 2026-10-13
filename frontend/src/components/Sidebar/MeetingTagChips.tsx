'use client';

import { meetingRowTags, type Tag } from '@/lib/organization';

const MAX_ROW_TAGS = 3;

interface MeetingTagChipsProps {
  tagIds: string[];
  tags: Tag[];
  selectedIds: string[];
  onToggle: (tagId: string) => void;
}

export function MeetingTagChips({ tagIds, tags, selectedIds, onToggle }: MeetingTagChipsProps) {
  if (tagIds.length === 0) return null;
  const { shown, hidden } = meetingRowTags(tagIds, tags, MAX_ROW_TAGS);
  if (shown.length === 0) return null;
  return (
    <div className="ml-8 mt-1 flex flex-wrap items-center gap-1 font-normal">
      {shown.map(tag => {
        const selected = selectedIds.includes(tag.id);
        return (
          <button
            key={tag.id}
            type="button"
            aria-pressed={selected}
            title={selected ? `Stop filtering by #${tag.name}` : `Filter by #${tag.name}`}
            onClick={e => {
              e.stopPropagation();
              onToggle(tag.id);
            }}
            className={`max-w-[9rem] truncate rounded-full border px-1.5 text-[11px] leading-4 ${
              selected
                ? 'border-blue-300 bg-blue-100 text-blue-800'
                : 'border-gray-200 bg-white text-gray-500 hover:bg-gray-100'
            }`}
          >
            #{tag.name}
          </button>
        );
      })}
      {hidden.length > 0 && (
        <span className="text-[11px] leading-4 text-gray-400" title={hidden.map(t => `#${t.name}`).join(' ')}>
          +{hidden.length}
        </span>
      )}
    </div>
  );
}
