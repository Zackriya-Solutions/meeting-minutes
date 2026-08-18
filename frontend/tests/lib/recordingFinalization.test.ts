import { describe, expect, test } from "bun:test";
import { decideRecordingPersistence } from "../../src/lib/recordingFinalization";

describe("decideRecordingPersistence", () => {
  test("saves only after transcription completion", () => {
    expect(decideRecordingPersistence(true, true)).toBe("save");
  });

  test("preserves an incomplete recording for recovery instead of saving partial history", () => {
    expect(decideRecordingPersistence(true, false)).toBe("preserve_for_recovery");
  });

  test("skips persistence when the caller requested shutdown without a database save", () => {
    expect(decideRecordingPersistence(false, true)).toBe("skip");
    expect(decideRecordingPersistence(false, false)).toBe("skip");
  });
});
