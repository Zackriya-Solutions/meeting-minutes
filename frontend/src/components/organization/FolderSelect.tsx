'use client';

import { Folder as FolderIcon } from 'lucide-react';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { UNFILED_ID, type Folder } from '@/lib/organization';

interface FolderSelectProps {
  folders: Folder[];
  value: string | null;
  onChange: (folderId: string | null) => void;
}

export function FolderSelect({ folders, value, onChange }: FolderSelectProps) {
  return (
    <Select value={value ?? UNFILED_ID} onValueChange={v => onChange(v === UNFILED_ID ? null : v)}>
      <SelectTrigger className="h-7 w-auto gap-1 px-2 text-xs" aria-label="Folder">
        <FolderIcon className="h-3 w-3" />
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        <SelectItem value={UNFILED_ID}>Unfiled</SelectItem>
        {folders.map(f => (
          <SelectItem key={f.id} value={f.id}>{f.name}</SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}
