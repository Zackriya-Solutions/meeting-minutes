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

const STARTABLE_STATUSES = new Set<RecordingStatus>([
  RecordingStatus.IDLE,
  RecordingStatus.COMPLETED,
  RecordingStatus.ERROR,
]);

/**
 * Whether a new recording may be started right now.
 *
 * Both inputs are consulted on purpose: `isRecording` can already be false while the
 * recorder is still stopping, saving, or processing transcripts, and starting a second
 * recording over that would race the first one's teardown.
 */
export function canStartRecordingNow(
  isRecording: boolean,
  status: RecordingStatus,
): boolean {
  return !isRecordingNavigationLocked(isRecording, status)
    && STARTABLE_STATUSES.has(status);
}
