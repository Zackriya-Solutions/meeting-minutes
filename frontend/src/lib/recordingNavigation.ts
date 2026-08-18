import { RecordingStatus } from '@/contexts/RecordingStateContext';

const BUSY_STATUSES = new Set<RecordingStatus>([
  RecordingStatus.STARTING,
  RecordingStatus.RECORDING,
  RecordingStatus.STOPPING,
  RecordingStatus.PROCESSING_TRANSCRIPTS,
  RecordingStatus.SAVING,
]);

/**
 * Whether the recorder is mid-session: live, spinning up, or still tearing down.
 *
 * This deliberately does NOT gate navigation. Capture and transcript accumulation
 * are owned by the layout-level `RecordingStateProvider`/`TranscriptProvider`, above
 * the router, so a recording survives the user walking off to settings or to an
 * older meeting. It only answers "is the recorder occupied right now".
 */
export function isRecordingSessionBusy(
  isRecording: boolean,
  status: RecordingStatus,
): boolean {
  return isRecording || BUSY_STATUSES.has(status);
}

/** Keep the live transcript visible until capture and post-processing are both settled. */
export function canDismissRecordingDrawer(
  isRecording: boolean,
  status: RecordingStatus,
): boolean {
  return !isRecordingSessionBusy(isRecording, status);
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
  return !isRecordingSessionBusy(isRecording, status)
    && STARTABLE_STATUSES.has(status);
}
