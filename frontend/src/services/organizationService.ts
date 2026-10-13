import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { Folder, PurgeOutcome, Tag, TagRecord, TrashItem } from '@/lib/organization';

export const organizationService = {
  listFolders: () => invoke<Folder[]>('list_folders'),
  createFolder: (name: string) => invoke<Folder>('create_folder', { name }),
  renameFolder: (id: string, name: string) => invoke<Folder>('rename_folder', { id, name }),
  deleteFolder: (id: string) => invoke<number>('delete_folder', { id }),

  listTags: () => invoke<Tag[]>('list_tags'),
  createTag: (name: string) => invoke<TagRecord>('create_tag', { name }),
  renameTag: (id: string, name: string) => invoke<TagRecord>('rename_tag', { id, name }),
  deleteTag: (id: string) => invoke<void>('delete_tag', { id }),

  setMeetingFolder: (meetingId: string, folderId: string | null) =>
    invoke<void>('set_meeting_folder', { meetingId, folderId }),
  setMeetingTags: (meetingId: string, tagIds: string[]) =>
    invoke<string[]>('set_meeting_tags', { meetingId, tagIds }),

  trashMeeting: (meetingId: string) => invoke<void>('trash_meeting', { meetingId }),
  restoreMeeting: (meetingId: string) => invoke<void>('restore_meeting', { meetingId }),
  deleteMeetingForever: (meetingId: string) => invoke<void>('delete_meeting_forever', { meetingId }),
  emptyTrash: () => invoke<PurgeOutcome>('empty_trash'),
  listTrash: () => invoke<TrashItem[]>('list_trash'),

  /** Fires after any meeting, folder, tag or Trash change. */
  onMeetingsChanged: (callback: () => void): Promise<UnlistenFn> =>
    listen('meetings-changed', () => callback()),
};
