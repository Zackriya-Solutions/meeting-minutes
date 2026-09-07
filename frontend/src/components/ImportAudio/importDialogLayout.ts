export interface ImportAudioDialogLayoutClasses {
  dialogContentClassName: string;
  contentClassName: string;
  filenameClassName: string;
  titleInputClassName: string;
  footerClassName: string;
}

export function getImportAudioDialogLayoutClasses(): ImportAudioDialogLayoutClasses {
  return {
    dialogContentClassName: 'w-[calc(100vw-2rem)] max-w-[500px] overflow-hidden',
    contentClassName: 'space-y-4 py-4 max-w-full',
    filenameClassName: 'font-medium text-gray-900 truncate w-full overflow-hidden text-ellipsis whitespace-nowrap',
    titleInputClassName: 'w-full min-w-0',
    footerClassName: 'flex-shrink-0',
  };
}
