import { describe, expect, test } from "bun:test";
import { RecordingStatus } from "../../src/contexts/RecordingStateContext";
import {
  canDismissRecordingDrawer,
  canStartRecordingNow,
  isRecordingSessionBusy,
} from "../../src/lib/recordingNavigation";

const BUSY_STATUSES = [
  RecordingStatus.STARTING,
  RecordingStatus.RECORDING,
  RecordingStatus.STOPPING,
  RecordingStatus.PROCESSING_TRANSCRIPTS,
  RecordingStatus.SAVING,
];

describe("canStartRecordingNow", () => {
  test("allows a start from every settled status", () => {
    for (const status of [
      RecordingStatus.IDLE,
      RecordingStatus.COMPLETED,
      RecordingStatus.ERROR,
    ]) {
      expect(canStartRecordingNow(false, status)).toBe(true);
    }
  });

  test("refuses while the recorder is working, including after isRecording drops", () => {
    // The teardown statuses are the point: `isRecording` goes false before saving and
    // transcript processing finish, and a second start there would race the first.
    for (const status of BUSY_STATUSES) {
      expect(canStartRecordingNow(false, status)).toBe(false);
    }
  });

  test("refuses whenever a recording is live, whatever the status says", () => {
    for (const status of Object.values(RecordingStatus)) {
      expect(canStartRecordingNow(true, status)).toBe(false);
    }
  });

  test("is never true while the recorder is busy", () => {
    for (const status of Object.values(RecordingStatus)) {
      for (const isRecording of [true, false]) {
        if (isRecordingSessionBusy(isRecording, status)) {
          expect(canStartRecordingNow(isRecording, status)).toBe(false);
        }
      }
    }
  });
});

describe("canDismissRecordingDrawer", () => {
  test("keeps the transcript drawer open for the full recording lifecycle", () => {
    for (const status of BUSY_STATUSES) {
      expect(canDismissRecordingDrawer(false, status)).toBe(false);
    }
    expect(canDismissRecordingDrawer(true, RecordingStatus.IDLE)).toBe(false);
  });

  test("allows dismissal after the session settles", () => {
    for (const status of [
      RecordingStatus.IDLE,
      RecordingStatus.COMPLETED,
      RecordingStatus.ERROR,
    ]) {
      expect(canDismissRecordingDrawer(false, status)).toBe(true);
    }
  });
});
