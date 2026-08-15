import { describe, expect, test } from "bun:test";
import { isDefaultTitle } from "../../src/lib/title-utils";

describe("isDefaultTitle", () => {
  test("treats null/undefined/empty as default", () => {
    expect(isDefaultTitle(null)).toBe(true);
    expect(isDefaultTitle(undefined)).toBe(true);
    expect(isDefaultTitle("")).toBe(true);
    expect(isDefaultTitle("   ")).toBe(true);
  });

  test("treats short titles as default", () => {
    expect(isDefaultTitle("abc")).toBe(true);
    expect(isDefaultTitle("Hi")).toBe(true);
  });

  test("treats 'Meeting ' prefixed titles as default", () => {
    expect(isDefaultTitle("Meeting 2024-01-01")).toBe(true);
  });

  test("treats 'meeting_' prefixed titles as default", () => {
    expect(isDefaultTitle("meeting_abc123")).toBe(true);
  });

  test("treats pure numeric/timestamp strings as default", () => {
    expect(isDefaultTitle("2024-01-15 09:30:00")).toBe(true);
    expect(isDefaultTitle("20240115T093000")).toBe(true);
    expect(isDefaultTitle("12345")).toBe(true);
  });

  test("accepts real user-assigned titles", () => {
    expect(isDefaultTitle("Sprint Planning")).toBe(false);
    expect(isDefaultTitle("Weekly Sync with Alice")).toBe(false);
    expect(isDefaultTitle("Q2 Roadmap Review")).toBe(false);
  });

  test("rejects titles with letters even if they contain digits", () => {
    expect(isDefaultTitle("Standup 01")).toBe(false);
    expect(isDefaultTitle("Sync-2024")).toBe(false);
  });
});
