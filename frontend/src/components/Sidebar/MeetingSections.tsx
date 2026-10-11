'use client';

import { useEffect, useState, type ReactNode } from 'react';
import { ChevronDown, ChevronRight, Folder as FolderIcon, Inbox, MoreHorizontal } from 'lucide-react';
import {
  DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { UNFILED_ID, type Folder, type MeetingListItem, type SidebarSection } from '@/lib/organization';

const DRAG_TYPE = 'application/x-meetily-meeting';
const COLLAPSED_KEY = 'sidebar-collapsed-sections';

function loadCollapsed(): Set<string> {
  try {
    return new Set(JSON.parse(localStorage.getItem(COLLAPSED_KEY) ?? '[]'));
  } catch {
    return new Set();
  }
}

interface MeetingSectionsProps {
  sections: SidebarSection[];
  renderMeeting: (meeting: MeetingListItem) => ReactNode;
  onDropMeeting: (meetingId: string, folderId: string | null) => void;
  onRenameFolder: (folder: Folder) => void;
  onDeleteFolder: (folder: Folder) => void;
}

export function MeetingSections({
  sections, renderMeeting, onDropMeeting, onRenameFolder, onDeleteFolder,
}: MeetingSectionsProps) {
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
  const [dropTarget, setDropTarget] = useState<string | null>(null);

  useEffect(() => setCollapsed(loadCollapsed()), []);

  const toggle = (id: string) => {
    const next = new Set(collapsed);
    if (next.has(id)) next.delete(id); else next.add(id);
    setCollapsed(next);
    try {
      localStorage.setItem(COLLAPSED_KEY, JSON.stringify([...next]));
    } catch {
      // Collapse state is a convenience; ignore storage failures.
    }
  };

  return (
    <div className="mx-3">
      {sections.map(section => {
        const isCollapsed = collapsed.has(section.id);
        const folderId = section.id === UNFILED_ID ? null : section.id;
        return (
          <div key={section.id} className="mb-1">
            <div
              className={`group flex items-center rounded-md px-2 py-1.5 text-sm font-medium text-gray-700 cursor-pointer ${
                dropTarget === section.id ? 'bg-blue-50 ring-1 ring-blue-300' : 'hover:bg-gray-50'
              }`}
              onClick={() => toggle(section.id)}
              onDragOver={e => {
                if (e.dataTransfer.types.includes(DRAG_TYPE)) {
                  e.preventDefault();
                  setDropTarget(section.id);
                }
              }}
              onDragLeave={() => setDropTarget(null)}
              onDrop={e => {
                e.preventDefault();
                setDropTarget(null);
                const meetingId = e.dataTransfer.getData(DRAG_TYPE);
                if (meetingId) onDropMeeting(meetingId, folderId);
              }}
            >
              {isCollapsed ? <ChevronRight className="mr-1 h-4 w-4" /> : <ChevronDown className="mr-1 h-4 w-4" />}
              {section.folder ? <FolderIcon className="mr-2 h-4 w-4 text-gray-500" /> : <Inbox className="mr-2 h-4 w-4 text-gray-500" />}
              <span className="flex-1 truncate">{section.title}</span>
              <span className="ml-2 text-xs text-gray-400">{section.meetings.length}</span>
              {section.folder && (
                <DropdownMenu>
                  <DropdownMenuTrigger asChild>
                    <button
                      type="button"
                      onClick={e => e.stopPropagation()}
                      className="ml-1 rounded p-0.5 opacity-0 hover:bg-gray-200 group-hover:opacity-100"
                      aria-label={`Folder actions for ${section.title}`}
                    >
                      <MoreHorizontal className="h-4 w-4" />
                    </button>
                  </DropdownMenuTrigger>
                  <DropdownMenuContent align="end" onClick={e => e.stopPropagation()}>
                    <DropdownMenuItem onSelect={() => onRenameFolder(section.folder!)}>Rename</DropdownMenuItem>
                    <DropdownMenuItem onSelect={() => onDeleteFolder(section.folder!)} className="text-red-600">
                      Delete folder
                    </DropdownMenuItem>
                  </DropdownMenuContent>
                </DropdownMenu>
              )}
            </div>
            {!isCollapsed && (
              <div className="ml-2">
                {section.meetings.map(meeting => (
                  <div
                    key={meeting.id}
                    draggable
                    onDragStart={e => {
                      e.dataTransfer.setData(DRAG_TYPE, meeting.id);
                      e.dataTransfer.effectAllowed = 'move';
                    }}
                  >
                    {renderMeeting(meeting)}
                  </div>
                ))}
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
}
