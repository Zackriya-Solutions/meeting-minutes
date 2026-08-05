import { describe, expect, test } from "bun:test";
import { RecordingStatus } from "../../src/contexts/RecordingStateContext";
import {
  RECORDING_ROUTE,
  canStartRecordingNow,
  isRecordingSessionBusy,
  shouldShowRecordingPill,
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

describe("shouldShowRecordingPill", () => {
  test("shows off-route while the recorder is busy, capture or teardown", () => {
    // Teardown matters most: `isRecording` is already false while the meeting saves,
    // and dropping the pill there would look like the recording vanished.
    for (const status of BUSY_STATUSES) {
      expect(shouldShowRecordingPill("/settings", false, status)).toBe(true);
      expect(shouldShowRecordingPill("/meeting-details", false, status)).toBe(true);
    }
  });

  test("shows off-route whenever a recording is live", () => {
    for (const status of Object.values(RecordingStatus)) {
      expect(shouldShowRecordingPill("/", true, status)).toBe(true);
    }
  });

  test("stays hidden on the recording route, which has its own controls", () => {
    for (const status of Object.values(RecordingStatus)) {
      for (const isRecording of [true, false]) {
        expect(shouldShowRecordingPill(RECORDING_ROUTE, isRecording, status)).toBe(false);
      }
    }
  });

  test("stays hidden when no session is running", () => {
    for (const status of [
      RecordingStatus.IDLE,
      RecordingStatus.COMPLETED,
      RecordingStatus.ERROR,
    ]) {
      expect(shouldShowRecordingPill("/settings", false, status)).toBe(false);
    }
  });
});
