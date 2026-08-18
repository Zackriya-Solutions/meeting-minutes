export interface TreeMeeting {
  id: string;
  title: string;
  project_folder_id?: string | null;
  tags?: string[];
}

export interface TreeFolder {
  id: string;
  name: string;
}

export interface TreeItem {
  id: string;
  title: string;
  type: 'folder' | 'file';
  children?: TreeItem[];
  tags?: string[];
  project_folder_id?: string | null;
}

export function buildMeetingTree(
  meetings: TreeMeeting[],
  projectFolders: TreeFolder[],
  unfiledFolderValue: string
): TreeItem[] {
  const knownFolderIds = new Set(projectFolders.map(folder => folder.id));
  const toMeetingItem = (meeting: TreeMeeting): TreeItem => ({
    id: meeting.id,
    title: meeting.title,
    type: 'file',
    tags: meeting.tags,
    project_folder_id: meeting.project_folder_id ?? null,
  });

  return [
    {
      id: 'meetings',
      title: 'Meeting Notes',
      type: 'folder',
      children: [
        {
          id: unfiledFolderValue,
          title: 'Unfiled',
          type: 'folder',
          children: meetings
            .filter(meeting => !meeting.project_folder_id || !knownFolderIds.has(meeting.project_folder_id))
            .map(toMeetingItem),
        },
        ...projectFolders.map(folder => ({
          id: folder.id,
          title: folder.name,
          type: 'folder' as const,
          children: meetings.filter(meeting => meeting.project_folder_id === folder.id).map(toMeetingItem),
        })),
      ],
    },
  ];
}
