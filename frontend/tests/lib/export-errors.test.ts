import { describe, expect, test } from "bun:test";
import {
  classifyError,
  shouldRetry,
  MAX_RETRIES,
} from "../../src/lib/export-errors";

describe("classifyError", () => {
  test("classifies user cancellation", () => {
    const e = classifyError("User cancelled export", 0);
    expect(e.kind).toBe("cancelled");
    expect(e.attempt).toBe(0);
  });

  test("classifies permission errors (permission denied)", () => {
    const e = classifyError("Failed to write file: Permission denied (os error 13)", 1);
    expect(e.kind).toBe("permission");
  });

  test("classifies permission errors (EACCES)", () => {
    const e = classifyError("EACCES: permission denied", 0);
    expect(e.kind).toBe("permission");
  });

  test("classifies read-only filesystem", () => {
    const e = classifyError("Read-only file system", 0);
    expect(e.kind).toBe("permission");
  });

  test("classifies not found (meeting missing)", () => {
    const e = classifyError("Meeting abc not found", 0);
    expect(e.kind).toBe("not_found");
  });

  test("classifies not found (no such file)", () => {
    const e = classifyError("No such file or directory", 0);
    expect(e.kind).toBe("not_found");
  });

  test("falls back to unknown for generic errors", () => {
    const e = classifyError("something exploded", 0);
    expect(e.kind).toBe("unknown");
  });

  test("preserves original message text", () => {
    const e = classifyError("wrapped: Meeting gone", 2);
    expect(e.message).toBe("wrapped: Meeting gone");
  });
});

describe("shouldRetry", () => {
  test("never retries cancellations", () => {
    const e = classifyError("User cancelled export", 0);
    expect(shouldRetry(e)).toBe(false);
  });

  test("retries transient errors below MAX_RETRIES", () => {
    const e = classifyError("network hiccup", 0);
    expect(shouldRetry(e)).toBe(true);
    const e2 = classifyError("network hiccup", MAX_RETRIES - 1);
    expect(shouldRetry(e2)).toBe(true);
  });

  test("stops retrying after MAX_RETRIES", () => {
    const e = classifyError("network hiccup", MAX_RETRIES);
    expect(shouldRetry(e)).toBe(false);
  });

  test("does not retry permission errors", () => {
    const e = classifyError("Permission denied", 0);
    expect(shouldRetry(e)).toBe(false);
  });

  test("does not retry not_found errors", () => {
    const e = classifyError("Meeting not found", 0);
    expect(shouldRetry(e)).toBe(false);
  });
});
