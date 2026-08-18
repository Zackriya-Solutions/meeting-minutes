import { beforeEach, describe, expect, mock, test } from "bun:test";

const calls: Array<{ command: string; args: Record<string, unknown> | undefined }> = [];

mock.module("@tauri-apps/api/core", () => ({
  invoke: async (command: string, args?: Record<string, unknown>) => {
    calls.push({ command, args });
    return [];
  },
}));

const { getUpcomingLocalOutlookMeetings } = await import("../../src/lib/localOutlookCalendar");

describe("Outlook calendar read modes", () => {
  beforeEach(() => {
    calls.length = 0;
  });

  test("background calendar reads do not enumerate attendee names", async () => {
    await getUpcomingLocalOutlookMeetings(7, { force: true });

    expect(calls).toEqual([{
      command: "get_upcoming_local_outlook_meetings",
      args: { days: 7, includeAttendees: false },
    }]);
  });

  test("a selected meeting can explicitly request attendee enrichment", async () => {
    await getUpcomingLocalOutlookMeetings(7, { force: true, includeAttendees: true });

    expect(calls).toEqual([{
      command: "get_upcoming_local_outlook_meetings",
      args: { days: 7, includeAttendees: true },
    }]);
  });
});
