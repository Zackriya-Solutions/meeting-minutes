'use client';

import { useRef, useState } from 'react';
import { Pencil, Trash2 } from 'lucide-react';
import { toast } from 'sonner';
import { useSidebar } from '@/components/Sidebar/SidebarProvider';
import { organizationService } from '@/services/organizationService';
import type { Tag } from '@/lib/organization';

export function TagManager() {
  const { tags } = useSidebar();
  const [editing, setEditing] = useState<{ id: string; name: string } | null>(null);
  const [deleting, setDeleting] = useState<Tag | null>(null);
  const savingRef = useRef(false);
  const cancelRenameRef = useRef(false);

  // Blur is the only submit path: Enter blurs, Escape blurs with the cancel flag set.
  const onRenameBlur = async () => {
    if (cancelRenameRef.current) {
      cancelRenameRef.current = false;
      setEditing(null);
      return;
    }
    if (!editing || savingRef.current) return;
    const name = editing.name.trim();
    const original = tags.find(t => t.id === editing.id)?.name;
    if (!name || name === original) {
      setEditing(null);
      return;
    }
    savingRef.current = true;
    try {
      await organizationService.renameTag(editing.id, name);
      setEditing(null);
    } catch (error) {
      toast.error(String(error));
    } finally {
      savingRef.current = false;
    }
  };

  const confirmDelete = async () => {
    const tag = deleting;
    setDeleting(null);
    if (!tag) return;
    try {
      await organizationService.deleteTag(tag.id);
      toast.success(`Deleted #${tag.name}`);
    } catch (error) {
      toast.error(String(error));
    }
  };

  if (tags.length === 0) {
    return <p className="text-sm text-gray-500">No tags yet. Add tags from a meeting or the record screen.</p>;
  }

  return (
    <ul className="divide-y divide-gray-100">
      {tags.map(tag => (
        <li key={tag.id} className="flex items-center gap-2 py-2">
          {deleting?.id === tag.id ? (
            <>
              <span className="flex-1 text-sm text-gray-800">
                Delete #{tag.name}? Removes it from {tag.meeting_count} meeting{tag.meeting_count === 1 ? '' : 's'}; meetings are kept.
              </span>
              <button
                type="button"
                aria-label={`Confirm delete tag ${tag.name}`}
                onClick={() => void confirmDelete()}
                className="rounded bg-red-600 px-2 py-1 text-xs text-white hover:bg-red-700"
              >
                Delete
              </button>
              <button
                type="button"
                aria-label={`Cancel deleting tag ${tag.name}`}
                onClick={() => setDeleting(null)}
                className="rounded px-2 py-1 text-xs text-gray-600 hover:bg-gray-100"
              >
                Cancel
              </button>
            </>
          ) : (
            <>
            {editing?.id === tag.id ? (
              <input
                autoFocus
                value={editing.name}
                onChange={e => setEditing({ id: tag.id, name: e.target.value })}
                onKeyDown={e => {
                  if (e.key === 'Escape') cancelRenameRef.current = true;
                  if (e.key === 'Enter' || e.key === 'Escape') e.currentTarget.blur();
                }}
                onBlur={() => void onRenameBlur()}
                className="flex-1 rounded-md border border-gray-300 px-2 py-1 text-sm"
              />
            ) : (
              <span className="flex-1 truncate text-sm text-gray-800">#{tag.name}</span>
            )}
            <span className="text-xs text-gray-400">
              {tag.meeting_count} meeting{tag.meeting_count === 1 ? '' : 's'}
            </span>
            <button
              type="button"
              aria-label={`Rename tag ${tag.name}`}
              onClick={() => setEditing({ id: tag.id, name: tag.name })}
              className="rounded p-1 text-gray-600 hover:bg-gray-100"
            >
              <Pencil className="h-4 w-4" />
            </button>
            <button
              type="button"
              aria-label={`Delete tag ${tag.name}`}
              onClick={() => setDeleting(tag)}
              className="rounded p-1 text-red-600 hover:bg-red-50"
            >
              <Trash2 className="h-4 w-4" />
            </button>
            </>
          )}
        </li>
      ))}
    </ul>
  );
}
