import { beforeEach, describe, expect, test } from "bun:test";
import {
  AUTO_START_FLAG_KEY,
  AUTO_START_PARTICIPANTS_KEY,
  AUTO_START_TITLE_KEY,
  clearAutoStartRequest,
  requestAutoStart,
  takeRequestedMeetingParticipants,
  takeRequestedMeetingTitle,
} from "../../src/lib/autoStartRecording";

/** Minimal in-memory stand-in for `sessionStorage`. */
function fakeStorage(initial: Record<string, string> = {}): Storage {
  const entries = new Map(Object.entries(initial));
  return {
    get length() {
      return entries.size;
    },
    clear: () => entries.clear(),
    getItem: (key: string) => entries.get(key) ?? null,
    key: (index: number) => [...entries.keys()][index] ?? null,
    removeItem: (key: string) => {
      entries.delete(key);
    },
    setItem: (key: string, value: string) => {
      entries.set(key, value);
    },
  };
}

describe("requestAutoStart", () => {
  let storage: Storage;

  beforeEach(() => {
    storage = fakeStorage();
  });

  test("raises the flag on its own when no title is known", () => {
    requestAutoStart(storage);
    expect(storage.getItem(AUTO_START_FLAG_KEY)).toBe("true");
    expect(storage.getItem(AUTO_START_TITLE_KEY)).toBeNull();
  });

  test("carries a calendar subject alongside the flag", () => {
    requestAutoStart(storage, "Weekly team sync");
    expect(storage.getItem(AUTO_START_FLAG_KEY)).toBe("true");
    expect(storage.getItem(AUTO_START_TITLE_KEY)).toBe("Weekly team sync");
  });

  test("trims a padded subject rather than storing the padding", () => {
    requestAutoStart(storage, "  Design review  ");
    expect(storage.getItem(AUTO_START_TITLE_KEY)).toBe("Design review");
  });

  test("an untitled request clears a title left by an earlier one", () => {
    requestAutoStart(storage, "Client call");
    requestAutoStart(storage);
    expect(storage.getItem(AUTO_START_TITLE_KEY)).toBeNull();
  });

  test("a blank title is treated as no title", () => {
    requestAutoStart(storage, "Client call");
    requestAutoStart(storage, "   ");
    expect(storage.getItem(AUTO_START_TITLE_KEY)).toBeNull();
  });

  test("carries the people the calendar entry invited", () => {
    requestAutoStart(storage, "Sprint planning", ["Андрей Евлампиев", "Мария Петрова"]);
    expect(JSON.parse(storage.getItem(AUTO_START_PARTICIPANTS_KEY)!)).toEqual([
      "Андрей Евлампиев",
      "Мария Петрова",
    ]);
  });

  test("a request without participants clears those left by an earlier one", () => {
    requestAutoStart(storage, "Client call", ["Мария Петрова"]);
    requestAutoStart(storage, "Untitled follow-up");
    expect(storage.getItem(AUTO_START_PARTICIPANTS_KEY)).toBeNull();
  });

  test("drops blanks and repeated spellings before storing", () => {
    requestAutoStart(storage, "Client call", [
      "  Мария Петрова  ",
      "",
      "мария петрова",
      "Гость",
    ]);
    expect(JSON.parse(storage.getItem(AUTO_START_PARTICIPANTS_KEY)!)).toEqual([
      "Мария Петрова",
      "Гость",
    ]);
  });
});

describe("takeRequestedMeetingParticipants", () => {
  test("returns the invited people once, then nothing", () => {
    const storage = fakeStorage();
    requestAutoStart(storage, "Sprint planning", ["Мария Петрова"]);

    expect(takeRequestedMeetingParticipants(storage)).toEqual(["Мария Петрова"]);
    expect(takeRequestedMeetingParticipants(storage)).toEqual([]);
  });

  test("is empty when the request carried nobody", () => {
    expect(takeRequestedMeetingParticipants(fakeStorage())).toEqual([]);
  });

  test("an unreadable stored value reads as empty and is cleared", () => {
    const storage = fakeStorage({ [AUTO_START_PARTICIPANTS_KEY]: "not json" });
    expect(takeRequestedMeetingParticipants(storage)).toEqual([]);
    expect(storage.getItem(AUTO_START_PARTICIPANTS_KEY)).toBeNull();
  });
});

describe("takeRequestedMeetingTitle", () => {
  test("returns the requested title and clears it, so the next start falls back", () => {
    const storage = fakeStorage();
    requestAutoStart(storage, "Sprint planning");

    expect(takeRequestedMeetingTitle(storage)).toBe("Sprint planning");
    expect(storage.getItem(AUTO_START_TITLE_KEY)).toBeNull();
    expect(takeRequestedMeetingTitle(storage)).toBeNull();
  });

  test("is null when nothing was requested", () => {
    expect(takeRequestedMeetingTitle(fakeStorage())).toBeNull();
  });

  test("a whitespace-only stored title reads as null and is still cleared", () => {
    const storage = fakeStorage({ [AUTO_START_TITLE_KEY]: "   " });
    expect(takeRequestedMeetingTitle(storage)).toBeNull();
    expect(storage.getItem(AUTO_START_TITLE_KEY)).toBeNull();
  });

  test("leaves the flag alone — consuming the title is not consuming the request", () => {
    const storage = fakeStorage();
    requestAutoStart(storage, "Product review");
    takeRequestedMeetingTitle(storage);
    expect(storage.getItem(AUTO_START_FLAG_KEY)).toBe("true");
  });
});

describe("clearAutoStartRequest", () => {
  test("withdraws every part of a pending request", () => {
    const storage = fakeStorage();
    requestAutoStart(storage, "Client call", ["Мария Петрова"]);

    clearAutoStartRequest(storage);

    expect(storage.getItem(AUTO_START_FLAG_KEY)).toBeNull();
    expect(storage.getItem(AUTO_START_TITLE_KEY)).toBeNull();
    expect(storage.getItem(AUTO_START_PARTICIPANTS_KEY)).toBeNull();
  });
});
