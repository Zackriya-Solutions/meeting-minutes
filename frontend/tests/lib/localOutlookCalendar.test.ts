import { describe, expect, test } from "bun:test";
import {
  MEETING_IN_PROGRESS_GRACE_MS,
  selectMeetingInProgress,
  type LocalOutlookMeeting,
} from "../../src/lib/localOutlookCalendar";

const NOON = new Date("2026-08-06T12:00:00Z").getTime();

function meeting(
  subject: string,
  startOffsetMinutes: number,
  endOffsetMinutes: number,
  overrides: Partial<LocalOutlookMeeting> = {},
): LocalOutlookMeeting {
  return {
    id: subject,
    calendar_id: "outlook-macos-calendar",
    calendar_name: "Календарь",
    store_name: "Microsoft Outlook for Mac",
    subject,
    start_at: new Date(NOON + startOffsetMinutes * 60_000).toISOString(),
    end_at: new Date(NOON + endOffsetMinutes * 60_000).toISOString(),
    is_all_day: false,
    is_meeting: true,
    is_recurring: false,
    location: null,
    response_status: "accepted",
    attendees: ["Мария Петрова"],
    ...overrides,
  };
}

describe("selectMeetingInProgress", () => {
  test("picks the entry covering now", () => {
    const meetings = [
      meeting("Прошедшая", -180, -120),
      meeting("Сейчас", -10, 50),
      meeting("Позже", 120, 180),
    ];
    expect(selectMeetingInProgress(meetings, NOON)?.subject).toBe("Сейчас");
  });

  test("prefers the one that started most recently when entries overlap", () => {
    const meetings = [
      meeting("Блок на весь день", -240, 240),
      meeting("Синк, который только начался", -2, 28),
    ];
    expect(selectMeetingInProgress(meetings, NOON)?.subject).toBe(
      "Синк, который только начался",
    );
  });

  test("matches a meeting the user joined a few minutes early or late", () => {
    const startingSoon = [meeting("Через три минуты", 3, 33)];
    expect(selectMeetingInProgress(startingSoon, NOON)?.subject).toBe("Через три минуты");

    const justEnded = [meeting("Закончилась минуту назад", -31, -1)];
    expect(selectMeetingInProgress(justEnded, NOON)?.subject).toBe(
      "Закончилась минуту назад",
    );
  });

  test("ignores an entry outside the grace window", () => {
    const graceMinutes = MEETING_IN_PROGRESS_GRACE_MS / 60_000;
    const meetings = [meeting("Позже", graceMinutes + 1, graceMinutes + 31)];
    expect(selectMeetingInProgress(meetings, NOON)).toBeNull();
  });

  test("never picks an all-day marker", () => {
    const meetings = [meeting("Отпуск", -240, 240, { is_all_day: true })];
    expect(selectMeetingInProgress(meetings, NOON)).toBeNull();
  });

  test("never picks a solo block, which shares the clock without being a call", () => {
    const meetings = [
      meeting("Фокус-время", -10, 50, { is_meeting: false, attendees: [] }),
    ];
    expect(selectMeetingInProgress(meetings, NOON)).toBeNull();
  });

  test("picks the call rather than the solo block it overlaps", () => {
    const meetings = [
      meeting("Фокус-время", -5, 55, { is_meeting: false, attendees: [] }),
      meeting("Синк с командой", -20, 40),
    ];
    expect(selectMeetingInProgress(meetings, NOON)?.subject).toBe("Синк с командой");
  });

  test("ignores an entry with unreadable times", () => {
    const meetings = [
      meeting("Сломанная", -10, 50, { start_at: "not a date", end_at: "not a date" }),
    ];
    expect(selectMeetingInProgress(meetings, NOON)).toBeNull();
  });

  test("an empty calendar has no meeting in progress", () => {
    expect(selectMeetingInProgress([], NOON)).toBeNull();
  });
});
