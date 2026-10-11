'use client';

import { useSidebar } from '@/components/Sidebar/SidebarProvider';
import { organizationService } from '@/services/organizationService';
import { FolderSelect } from './FolderSelect';
import { TagPicker } from './TagPicker';

export function PendingOrganizationRow() {
  const { folders, tags, pendingOrganization, setPendingOrganization, refetchOrganization } = useSidebar();
  return (
    <div className="mb-2 flex items-center justify-center gap-2">
      <FolderSelect
        folders={folders}
        value={pendingOrganization.folderId}
        onChange={folderId => setPendingOrganization({ ...pendingOrganization, folderId })}
      />
      <TagPicker
        tags={tags}
        selectedIds={pendingOrganization.tagIds}
        onChange={tagIds => setPendingOrganization({ ...pendingOrganization, tagIds })}
        onCreate={async name => {
          const tag = await organizationService.createTag(name);
          await refetchOrganization();
          return tag;
        }}
      />
    </div>
  );
}
