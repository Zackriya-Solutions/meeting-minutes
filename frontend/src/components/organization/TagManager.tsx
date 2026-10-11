'use client';

import { useState } from 'react';
import { Pencil, Trash2 } from 'lucide-react';
import { toast } from 'sonner';
import { ConfirmationModal } from '@/components/ConfirmationModel/confirmation-modal';
import { useSidebar } from '@/components/Sidebar/SidebarProvider';
import { organizationService } from '@/services/organizationService';
import type { Tag } from '@/lib/organization';

export function TagManager() {
  const { tags, refetchMeetings, refetchOrganization } = useSidebar();
  const [editing, setEditing] = useState<{ id: string; name: string } | null>(null);
  const [deleting, setDeleting] = useState<Tag | null>(null);

  const refresh = () => Promise.all([refetchMeetings(), refetchOrganization()]);

  const saveRename = async () => {
    if (!editing) return;
    const name = editing.name.trim();
    const original = tags.find(t => t.id === editing.id)?.name;
    if (!name || name === original) {
      setEditing(null);
      return;
    }
    try {
      await organizationService.renameTag(editing.id, name);
      setEditing(null);
      await refresh();
    } catch (error) {
      toast.error(String(error));
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
    await refresh();
  };

  if (tags.length === 0) {
    return <p className="text-sm text-gray-500">No tags yet. Add tags from a meeting or the record screen.</p>;
  }

  return (
    <>
      <ul className="divide-y divide-gray-100">
        {tags.map(tag => (
          <li key={tag.id} className="flex items-center gap-2 py-2">
            {editing?.id === tag.id ? (
              <input
                autoFocus
                value={editing.name}
                onChange={e => setEditing({ id: tag.id, name: e.target.value })}
                onKeyDown={e => {
                  if (e.key === 'Enter') void saveRename();
                  if (e.key === 'Escape') setEditing(null);
                }}
                onBlur={() => void saveRename()}
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
          </li>
        ))}
      </ul>
      <ConfirmationModal
        isOpen={deleting !== null}
        text={deleting
          ? `Delete tag #${deleting.name}? It will be removed from ${deleting.meeting_count} meeting(s). The meetings themselves are kept.`
          : ''}
        onConfirm={() => void confirmDelete()}
        onCancel={() => setDeleting(null)}
      />
    </>
  );
}
