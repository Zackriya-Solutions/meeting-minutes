'use client';

import { Check, FolderPlus, MoreHorizontal } from 'lucide-react';
import {
  DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuSeparator,
  DropdownMenuSub, DropdownMenuSubContent, DropdownMenuSubTrigger, DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import type { Folder } from '@/lib/organization';

interface MeetingItemMenuProps {
  folders: Folder[];
  folderId: string | null;
  onMove: (folderId: string | null) => void;
  onNewFolder: () => void;
  onEditTags: () => void;
  onTrash: () => void;
}

export function MeetingItemMenu({ folders, folderId, onMove, onNewFolder, onEditTags, onTrash }: MeetingItemMenuProps) {
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <button
          type="button"
          onClick={e => e.stopPropagation()}
          className="rounded p-1 hover:bg-gray-200"
          aria-label="Meeting actions"
        >
          <MoreHorizontal className="h-4 w-4" />
        </button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" onClick={e => e.stopPropagation()}>
        <DropdownMenuSub>
          <DropdownMenuSubTrigger>Move to folder</DropdownMenuSubTrigger>
          <DropdownMenuSubContent>
            <DropdownMenuItem onSelect={() => onMove(null)}>
              <Check className={`mr-2 h-3 w-3 ${folderId === null ? 'opacity-100' : 'opacity-0'}`} />
              Unfiled
            </DropdownMenuItem>
            {folders.map(f => (
              <DropdownMenuItem key={f.id} onSelect={() => onMove(f.id)}>
                <Check className={`mr-2 h-3 w-3 ${folderId === f.id ? 'opacity-100' : 'opacity-0'}`} />
                {f.name}
              </DropdownMenuItem>
            ))}
            <DropdownMenuSeparator />
            <DropdownMenuItem onSelect={onNewFolder}>
              <FolderPlus className="mr-2 h-3 w-3" />
              New folder…
            </DropdownMenuItem>
          </DropdownMenuSubContent>
        </DropdownMenuSub>
        <DropdownMenuItem onSelect={onEditTags}>Edit tags…</DropdownMenuItem>
        <DropdownMenuSeparator />
        <DropdownMenuItem onSelect={onTrash} className="text-red-600">Move to Trash</DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
