import { describe, expect, test } from "bun:test";
import { isSummaryRunStalled, STALLED_SUMMARY_AFTER_MS } from "../../src/lib/summaryRunProgress";

const NOW = Date.parse("2026-08-06T09:00:00.000Z");
const minutesAgo = (minutes: number) => new Date(NOW - minutes * 60_000).toISOString();

describe("isSummaryRunStalled", () => {
  test("keeps waiting on a run that is still within the poller's own timeout", () => {
    expect(isSummaryRunStalled({ startedAt: minutesAgo(0), firstSeenAt: NOW, now: NOW })).toBe(false);
    // A slow local model is allowed to take longer than the 16.5-minute poll cap.
    expect(isSummaryRunStalled({ startedAt: minutesAgo(19), firstSeenAt: NOW, now: NOW })).toBe(false);
  });

  test("gives up on a run left behind by an app that exited mid-generation", () => {
    // The case that stranded the meeting screen: a row pending since the previous session.
    expect(isSummaryRunStalled({ startedAt: minutesAgo(60 * 15), firstSeenAt: NOW, now: NOW })).toBe(true);
    expect(isSummaryRunStalled({ startedAt: minutesAgo(21), firstSeenAt: NOW, now: NOW })).toBe(true);
  });

  test("falls back to the first sighting when the run reports no start time", () => {
    const firstSeenAt = NOW - STALLED_SUMMARY_AFTER_MS - 1;
    expect(isSummaryRunStalled({ startedAt: null, firstSeenAt: NOW, now: NOW })).toBe(false);
    expect(isSummaryRunStalled({ startedAt: null, firstSeenAt, now: NOW })).toBe(true);
    // An unparseable timestamp is no better than a missing one and must not read as "brand new".
    expect(isSummaryRunStalled({ startedAt: "not a date", firstSeenAt, now: NOW })).toBe(true);
  });
});
