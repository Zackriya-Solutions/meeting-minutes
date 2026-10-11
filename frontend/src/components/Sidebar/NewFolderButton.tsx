'use client';

import { useState } from 'react';
import { FolderPlus } from 'lucide-react';
import { toast } from 'sonner';
import { organizationService } from '@/services/organizationService';

export function NewFolderButton({ onCreated }: { onCreated: () => Promise<void> }) {
  const [editing, setEditing] = useState(false);
  const [name, setName] = useState('');

  const submit = async () => {
    if (!name.trim()) {
      setEditing(false);
      return;
    }
    try {
      await organizationService.createFolder(name);
      setName('');
      setEditing(false);
      await onCreated();
    } catch (error) {
      toast.error(String(error));
    }
  };

  if (editing) {
    return (
      <input
        autoFocus
        value={name}
        onChange={e => setName(e.target.value)}
        onKeyDown={e => {
          if (e.key === 'Enter') void submit();
          if (e.key === 'Escape') { setName(''); setEditing(false); }
        }}
        onBlur={() => void submit()}
        placeholder="Folder name"
        className="mx-3 my-1 w-[calc(100%-1.5rem)] rounded-md border border-gray-300 px-2 py-1 text-sm"
      />
    );
  }
  return (
    <button
      type="button"
      onClick={() => setEditing(true)}
      className="mx-3 my-1 flex items-center gap-2 rounded-md px-3 py-1.5 text-sm text-gray-500 hover:bg-gray-50"
    >
      <FolderPlus className="h-4 w-4" /> New folder
    </button>
  );
}
