'use client';

import React, { Suspense, useMemo, useState } from 'react';
import { useRouter, useSearchParams } from 'next/navigation';
import { format, isToday, isYesterday } from 'date-fns';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import { File, Folder as FolderIcon, FolderMinus, MoreHorizontal, NotebookPen, Pencil, Trash2 } from 'lucide-react';

import { useSidebar } from '@/components/Sidebar/SidebarProvider';
import type { CurrentMeeting } from '@/components/Sidebar/SidebarProvider';
import { ConfirmationModal } from '@/components/ConfirmationModel/confirmation-modal';
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogTitle,
} from '@/components/ui/dialog';
import { VisuallyHidden } from '@/components/ui/visually-hidden';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import Analytics from '@/lib/analytics';

// Group label in the style of Granola's My Notes list
function groupLabel(date: Date): string {
  if (isToday(date)) return 'Today';
  if (isYesterday(date)) return 'Yesterday';
  if (date.getFullYear() === new Date().getFullYear()) return format(date, 'EEE, MMM d');
  return format(date, 'MMM d, yyyy');
}

interface MeetingGroup {
  key: string;
  label: string;
  meetings: CurrentMeeting[];
}

function NotesContent() {
  const router = useRouter();
  const searchParams = useSearchParams();
  const folderId = searchParams.get('folder');
  const isUncategorized = searchParams.get('view') === 'uncategorized';

  const {
    meetings,
    folders,
    setCurrentMeeting,
    moveMeetingToFolder,
    refetchMeetings,
    refetchFolders,
  } = useSidebar();

  const [renameState, setRenameState] = useState<{ isOpen: boolean; meetingId: string | null }>({ isOpen: false, meetingId: null });
  const [renameTitle, setRenameTitle] = useState('');
  const [deleteState, setDeleteState] = useState<{ isOpen: boolean; meetingId: string | null }>({ isOpen: false, meetingId: null });

  const activeFolder = !isUncategorized && folderId ? folders.find(f => f.id === folderId) ?? null : null;
  const folderNameById = useMemo(() => new Map(folders.map(f => [f.id, f.name])), [folders]);

  // Meetings in scope, newest first, grouped by calendar day
  const groups = useMemo<MeetingGroup[]>(() => {
    const inScope = activeFolder
      ? meetings.filter(m => m.folder_id === activeFolder.id)
      : isUncategorized
        ? meetings.filter(m => m.folder_id === null || m.folder_id === undefined)
      : meetings;

    const sorted = [...inScope].sort((a, b) =>
      new Date(b.created_at ?? 0).getTime() - new Date(a.created_at ?? 0).getTime()
    );

    const result: MeetingGroup[] = [];
    for (const meeting of sorted) {
      const date = meeting.created_at ? new Date(meeting.created_at) : null;
      const valid = date && !isNaN(date.getTime());
      const key = valid ? format(date!, 'yyyy-MM-dd') : 'undated';
      const label = valid ? groupLabel(date!) : 'Undated';

      const last = result[result.length - 1];
      if (last && last.key === key) {
        last.meetings.push(meeting);
      } else {
        result.push({ key, label, meetings: [meeting] });
      }
    }
    return result;
  }, [meetings, activeFolder, isUncategorized]);

  const meetingCount = groups.reduce((n, g) => n + g.meetings.length, 0);

  const openMeeting = (meeting: CurrentMeeting) => {
    setCurrentMeeting({ id: meeting.id, title: meeting.title });
    router.push(`/meeting-details?id=${meeting.id}`);
  };

  const handleMove = async (meetingId: string, targetFolderId: string | null) => {
    const ok = await moveMeetingToFolder(meetingId, targetFolderId);
    if (ok) {
      const name = targetFolderId ? folderNameById.get(targetFolderId) : null;
      toast.success(name ? `Moved to ${name}` : 'Removed from folder');
    } else {
      toast.error('Failed to move meeting');
    }
  };

  const handleRenameConfirm = async () => {
    const title = renameTitle.trim();
    const meetingId = renameState.meetingId;
    if (!meetingId) return;
    if (!title) {
      toast.error('Meeting title cannot be empty');
      return;
    }

    try {
      await invoke('api_save_meeting_title', { meetingId, title });
      await refetchMeetings();
      toast.success('Meeting title updated');
    } catch (error) {
      toast.error('Failed to update meeting title', {
        description: error instanceof Error ? error.message : String(error),
      });
    }
    setRenameState({ isOpen: false, meetingId: null });
    setRenameTitle('');
  };

  const handleDeleteConfirm = async () => {
    const meetingId = deleteState.meetingId;
    setDeleteState({ isOpen: false, meetingId: null });
    if (!meetingId) return;

    try {
      await invoke('api_delete_meeting', { meetingId });
      Analytics.trackMeetingDeleted(meetingId);
      await refetchMeetings();
      await refetchFolders();
      toast.success('Meeting deleted', { description: 'All associated data has been removed' });
    } catch (error) {
      toast.error('Failed to delete meeting', {
        description: error instanceof Error ? error.message : String(error),
      });
    }
  };

  return (
    <div className="flex flex-col h-screen bg-surface">
      <div className="flex-1 overflow-y-auto custom-scrollbar">
        <div className="max-w-3xl mx-auto px-8 py-12">
          {/* Header */}
          <div className="flex items-center gap-3">
            <div className="flex items-center justify-center w-10 h-10 rounded-xl bg-gray-100 border border-gray-200">
              {activeFolder || isUncategorized ? (
                <FolderIcon className="w-5 h-5 text-gray-500" />
              ) : (
                <NotebookPen className="w-5 h-5 text-gray-500" />
              )}
            </div>
            <div>
              <h1 className="text-3xl font-bold text-gray-900">
                {activeFolder ? activeFolder.name : isUncategorized ? 'Uncategorized' : 'All Notes'}
              </h1>
              <p className="text-sm text-gray-500 mt-0.5">
                {activeFolder
                  ? `${meetingCount} ${meetingCount === 1 ? 'meeting' : 'meetings'} in this folder`
                  : isUncategorized
                    ? `${meetingCount} ${meetingCount === 1 ? 'meeting' : 'meetings'} without a folder`
                  : `Notes from all of your meetings · ${meetingCount} ${meetingCount === 1 ? 'meeting' : 'meetings'}`}
              </p>
            </div>
          </div>

          {/* Empty states */}
          {meetingCount === 0 && (
            <div className="mt-16 text-center">
              <p className="text-gray-500">
                {activeFolder
                  ? 'No meetings in this folder yet.'
                  : isUncategorized
                    ? 'No uncategorized meetings yet.'
                  : 'No meetings yet.'}
              </p>
              <p className="text-sm text-gray-400 mt-1">
                {activeFolder
                  ? 'Use the ⋯ menu on any note to move it here.'
                  : isUncategorized
                    ? 'Meetings without a folder will appear here.'
                  : 'Start a recording to create your first meeting note.'}
              </p>
            </div>
          )}

          {/* Grouped list */}
          {groups.map(group => (
            <div key={group.key} className="mt-8">
              <div className="text-sm font-medium text-gray-500 mb-1 px-3">{group.label}</div>
              {group.meetings.map(meeting => {
                const date = meeting.created_at ? new Date(meeting.created_at) : null;
                const timeLabel = date && !isNaN(date.getTime()) ? format(date, 'h:mm a') : '';
                const folderName = !activeFolder && !isUncategorized && meeting.folder_id
                  ? folderNameById.get(meeting.folder_id)
                  : null;

                return (
                  <div
                    key={meeting.id}
                    onClick={() => openMeeting(meeting)}
                    className="flex items-center gap-3 px-3 py-2.5 rounded-xl hover:bg-gray-100 cursor-pointer group transition-colors"
                  >
                    <div className="flex-shrink-0 flex items-center justify-center w-9 h-9 rounded-lg bg-gray-100 border border-gray-200">
                      <File className="w-4 h-4 text-gray-500" />
                    </div>

                    <div className="flex-1 min-w-0">
                      <div className="text-[15px] font-medium text-gray-900 truncate">{meeting.title}</div>
                      {folderName && (
                        <div className="flex items-center gap-1 text-xs text-gray-500 mt-0.5">
                          <FolderIcon className="w-3 h-3" />
                          <span className="truncate">{folderName}</span>
                        </div>
                      )}
                    </div>

                    <DropdownMenu>
                      <DropdownMenuTrigger asChild>
                        <button
                          onClick={(e) => e.stopPropagation()}
                          className="p-1.5 rounded-md text-gray-400 hover:text-gray-700 hover:bg-gray-200 opacity-0 group-hover:opacity-100 data-[state=open]:opacity-100 transition-opacity"
                          aria-label="Meeting actions"
                        >
                          <MoreHorizontal className="w-4 h-4" />
                        </button>
                      </DropdownMenuTrigger>
                      <DropdownMenuContent align="end" onClick={(e) => e.stopPropagation()}>
                        <DropdownMenuSub>
                          <DropdownMenuSubTrigger>
                            <FolderIcon className="w-4 h-4 mr-2" />
                            Move to folder
                          </DropdownMenuSubTrigger>
                          <DropdownMenuSubContent>
                            {folders.length === 0 && (
                              <DropdownMenuItem disabled>No folders yet</DropdownMenuItem>
                            )}
                            {folders.map(folder => (
                              <DropdownMenuItem
                                key={folder.id}
                                disabled={meeting.folder_id === folder.id}
                                onClick={() => handleMove(meeting.id, folder.id)}
                              >
                                <FolderIcon className="w-4 h-4 mr-2" />
                                {folder.name}
                              </DropdownMenuItem>
                            ))}
                            {meeting.folder_id && (
                              <>
                                <DropdownMenuSeparator />
                                <DropdownMenuItem onClick={() => handleMove(meeting.id, null)}>
                                  <FolderMinus className="w-4 h-4 mr-2" />
                                  Remove from folder
                                </DropdownMenuItem>
                              </>
                            )}
                          </DropdownMenuSubContent>
                        </DropdownMenuSub>
                        <DropdownMenuItem
                          onClick={() => {
                            setRenameState({ isOpen: true, meetingId: meeting.id });
                            setRenameTitle(meeting.title);
                          }}
                        >
                          <Pencil className="w-4 h-4 mr-2" />
                          Rename
                        </DropdownMenuItem>
                        <DropdownMenuSeparator />
                        <DropdownMenuItem
                          className="text-red-500 focus:text-red-500"
                          onClick={() => setDeleteState({ isOpen: true, meetingId: meeting.id })}
                        >
                          <Trash2 className="w-4 h-4 mr-2" />
                          Delete
                        </DropdownMenuItem>
                      </DropdownMenuContent>
                    </DropdownMenu>

                    <span className="flex-shrink-0 text-xs text-gray-500 tabular-nums w-16 text-right">
                      {timeLabel}
                    </span>
                  </div>
                );
              })}
            </div>
          ))}
        </div>
      </div>

      {/* Delete confirmation */}
      <ConfirmationModal
        isOpen={deleteState.isOpen}
        text="Are you sure you want to delete this meeting? This action cannot be undone."
        onConfirm={handleDeleteConfirm}
        onCancel={() => setDeleteState({ isOpen: false, meetingId: null })}
      />

      {/* Rename dialog */}
      <Dialog open={renameState.isOpen} onOpenChange={(open) => {
        if (!open) {
          setRenameState({ isOpen: false, meetingId: null });
          setRenameTitle('');
        }
      }}>
        <DialogContent className="sm:max-w-[425px]">
          <VisuallyHidden>
            <DialogTitle>Rename Meeting</DialogTitle>
          </VisuallyHidden>
          <div className="py-4">
            <h3 className="text-lg font-semibold mb-4">Rename Meeting</h3>
            <input
              type="text"
              value={renameTitle}
              onChange={(e) => setRenameTitle(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter') handleRenameConfirm();
                if (e.key === 'Escape') {
                  setRenameState({ isOpen: false, meetingId: null });
                  setRenameTitle('');
                }
              }}
              className="w-full px-3 py-2 border border-gray-300 rounded-md focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent"
              placeholder="Enter meeting title"
              autoFocus
            />
          </div>
          <DialogFooter>
            <button
              onClick={() => {
                setRenameState({ isOpen: false, meetingId: null });
                setRenameTitle('');
              }}
              className="px-4 py-2 text-sm font-medium text-gray-700 bg-gray-100 hover:bg-gray-200 rounded-md transition-colors"
            >
              Cancel
            </button>
            <button
              onClick={handleRenameConfirm}
              className="px-4 py-2 text-sm font-medium text-white bg-blue-700 hover:bg-blue-600 rounded-md transition-colors"
            >
              Save
            </button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}

export default function NotesPage() {
  return (
    <Suspense fallback={<div className="flex items-center justify-center h-screen bg-surface" />}>
      <NotesContent />
    </Suspense>
  );
}
