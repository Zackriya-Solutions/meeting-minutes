'use client';

import { toast } from 'sonner';
import { useSidebar } from '@/components/Sidebar/SidebarProvider';
import { organizationService } from '@/services/organizationService';
import { FolderSelect } from './FolderSelect';
import { TagPicker } from './TagPicker';

export function MeetingOrganizationBar({ meetingId, className = '' }: { meetingId: string; className?: string }) {
  const { meetings, folders, tags, refetchMeetings, refetchOrganization } = useSidebar();
  const meeting = meetings.find(m => m.id === meetingId);
  if (!meeting) return null;

  const run = async (action: () => Promise<unknown>) => {
    try {
      await action();
    } catch (error) {
      toast.error(String(error));
    } finally {
      await Promise.all([refetchMeetings(), refetchOrganization()]);
    }
  };

  return (
    <div className={`flex items-center gap-2 ${className}`}>
      <FolderSelect
        folders={folders}
        value={meeting.folder_id}
        onChange={folderId => run(() => organizationService.setMeetingFolder(meetingId, folderId))}
      />
      <TagPicker
        tags={tags}
        selectedIds={meeting.tag_ids}
        onChange={tagIds => run(() => organizationService.setMeetingTags(meetingId, tagIds))}
        onCreate={async name => {
          const tag = await organizationService.createTag(name);
          await refetchOrganization();
          return tag;
        }}
      />
    </div>
  );
}
