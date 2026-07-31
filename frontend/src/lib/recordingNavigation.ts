import { RecordingStatus } from '@/contexts/RecordingStateContext';

const NAVIGATION_LOCKED_STATUSES = new Set<RecordingStatus>([
  RecordingStatus.STARTING,
  RecordingStatus.RECORDING,
  RecordingStatus.STOPPING,
  RecordingStatus.PROCESSING_TRANSCRIPTS,
  RecordingStatus.SAVING,
]);

export function isRecordingNavigationLocked(
  isRecording: boolean,
  status: RecordingStatus,
): boolean {
  return isRecording || NAVIGATION_LOCKED_STATUSES.has(status);
}
