'use client';

import { toast } from 'sonner';
import { useSidebar } from '@/components/Sidebar/SidebarProvider';
import { organizationService } from '@/services/organizationService';
import { FolderSelect } from './FolderSelect';
import { TagPicker } from './TagPicker';

const reportError = (error: unknown) => {
  toast.error(String(error));
};

export function MeetingOrganizationBar({ meetingId, className = '' }: { meetingId: string; className?: string }) {
  const { meetings, folders, tags } = useSidebar();
  const meeting = meetings.find(m => m.id === meetingId);
  if (!meeting) return null;

  return (
    <div className={`flex items-center gap-2 ${className}`}>
      <FolderSelect
        folders={folders}
        value={meeting.folder_id}
        onChange={folderId => void organizationService.setMeetingFolder(meetingId, folderId).catch(reportError)}
      />
      <TagPicker
        tags={tags}
        selectedIds={meeting.tag_ids}
        onChange={tagIds => void organizationService.setMeetingTags(meetingId, tagIds).catch(reportError)}
        onCreate={organizationService.createTag}
      />
    </div>
  );
}
