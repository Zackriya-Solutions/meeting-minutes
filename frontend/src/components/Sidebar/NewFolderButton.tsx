'use client';

import { useRef, useState } from 'react';
import { FolderPlus } from 'lucide-react';
import { toast } from 'sonner';
import { organizationService } from '@/services/organizationService';

export function NewFolderButton() {
  const [editing, setEditing] = useState(false);
  const [name, setName] = useState('');
  const submittingRef = useRef(false);
  const cancelRef = useRef(false);

  // Blur is the only submit path: Enter blurs, Escape blurs with the cancel flag set.
  const onBlur = async () => {
    if (cancelRef.current || !name.trim()) {
      cancelRef.current = false;
      setName('');
      setEditing(false);
      return;
    }
    if (submittingRef.current) return;
    submittingRef.current = true;
    try {
      await organizationService.createFolder(name);
      setName('');
      setEditing(false);
    } catch (error) {
      toast.error(String(error));
    } finally {
      submittingRef.current = false;
    }
  };

  if (editing) {
    return (
      <input
        autoFocus
        value={name}
        onChange={e => setName(e.target.value)}
        onKeyDown={e => {
          if (e.key === 'Escape') cancelRef.current = true;
          if (e.key === 'Enter' || e.key === 'Escape') e.currentTarget.blur();
        }}
        onBlur={() => void onBlur()}
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
