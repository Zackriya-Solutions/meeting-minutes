export type BuiltInModelStatusType =
  | 'not_downloaded'
  | 'downloading'
  | 'available'
  | 'corrupted'
  | 'error';

export interface BuiltInModelListItem {
  name: string;
  status?: {
    type?: BuiltInModelStatusType | string;
  } | null;
}

export function isSelectableBuiltInModelStatus(
  statusType: BuiltInModelStatusType | string | undefined,
  isDownloading = false
): boolean {
  return !isDownloading && (statusType === 'available' || statusType === 'not_downloaded');
}

export function getFirstSelectableBuiltInModelName(models: BuiltInModelListItem[]): string {
  const firstAvailable = models.find((model) => model.status?.type === 'available');
  const firstNotDownloaded = models.find((model) => model.status?.type === 'not_downloaded');

  return (firstAvailable ?? firstNotDownloaded)?.name ?? '';
}
