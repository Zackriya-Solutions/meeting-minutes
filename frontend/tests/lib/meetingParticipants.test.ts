import { describe, expect, mock, test } from "bun:test";

mock.module("@tauri-apps/api/core", () => ({
  invoke: async (command: string, args?: Record<string, unknown>) => {
    invocations.push({ command, args });
    return 0;
  },
}));

const invocations: Array<{ command: string; args: Record<string, unknown> | undefined }> = [];

const {
  PENDING_PARTICIPANTS_KEY,
  PENDING_ROSTER_KEY,
  SOURCE_MANUAL_ROSTER,
  readMeetingRoster,
  readMeetingParticipants,
  rememberMeetingParticipants,
  saveMeetingParticipants,
  takeMeetingParticipants,
  takeMeetingRoster,
  writeMeetingRoster,
} = await import("../../src/lib/meetingParticipants");

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

describe("the pending participants slot", () => {
  test("can be inspected without consuming it before a fallible meeting save", () => {
    const storage = fakeStorage();
    rememberMeetingParticipants(storage, ["Мария Петрова"]);

    expect(readMeetingParticipants(storage)).toEqual(["Мария Петрова"]);
    expect(takeMeetingParticipants(storage)).toEqual(["Мария Петрова"]);
  });

  test("survives from the start of a recording to the save that follows it", () => {
    const storage = fakeStorage();
    rememberMeetingParticipants(storage, ["Андрей Евлампиев", "Мария Петрова"]);

    expect(takeMeetingParticipants(storage)).toEqual([
      "Андрей Евлампиев",
      "Мария Петрова",
    ]);
  });

  test("is one-shot, so an unsaved recording cannot hand its invitees to the next", () => {
    const storage = fakeStorage();
    rememberMeetingParticipants(storage, ["Мария Петрова"]);

    takeMeetingParticipants(storage);

    expect(takeMeetingParticipants(storage)).toEqual([]);
    expect(storage.getItem(PENDING_PARTICIPANTS_KEY)).toBeNull();
  });

  test("a start with nobody invited clears what an earlier one left", () => {
    const storage = fakeStorage();
    rememberMeetingParticipants(storage, ["Мария Петрова"]);
    rememberMeetingParticipants(storage, []);

    expect(takeMeetingParticipants(storage)).toEqual([]);
  });

  test("an unreadable stored value reads as empty and is cleared", () => {
    const storage = fakeStorage({ [PENDING_PARTICIPANTS_KEY]: "{oops" });
    expect(takeMeetingParticipants(storage)).toEqual([]);
    expect(storage.getItem(PENDING_PARTICIPANTS_KEY)).toBeNull();
  });
});

describe("saveMeetingParticipants", () => {
  test("names the command arguments the way the command reads them", async () => {
    invocations.length = 0;
    await saveMeetingParticipants("meeting-1", ["Мария Петрова"]);

    expect(invocations).toHaveLength(1);
    expect(invocations[0]!.command).toBe("set_meeting_participants");
    expect(invocations[0]!.args).toEqual({
      meetingId: "meeting-1",
      participants: ["Мария Петрова"],
      source: "outlook_calendar",
    });
  });

  test("does not call the backend when nobody was invited", async () => {
    invocations.length = 0;
    await saveMeetingParticipants("meeting-1", []);
    await saveMeetingParticipants("meeting-1", ["   "]);

    expect(invocations).toEqual([]);
  });
});

describe("the roster typed during a recording", () => {
  test("survives a reload and keeps the order the user typed", () => {
    const storage = fakeStorage();
    writeMeetingRoster(storage, ["Миша", "Андрей"]);

    expect(readMeetingRoster(storage)).toEqual(["Миша", "Андрей"]);
    // Reading is not consuming: the recorder shows the roster back while it runs.
    expect(readMeetingRoster(storage)).toEqual(["Миша", "Андрей"]);
  });

  test("drops blanks and repeats, and clears the slot when emptied", () => {
    const storage = fakeStorage();
    writeMeetingRoster(storage, ["Миша", "  ", "миша", "Андрей"]);
    expect(readMeetingRoster(storage)).toEqual(["Миша", "Андрей"]);

    writeMeetingRoster(storage, []);
    expect(storage.getItem(PENDING_ROSTER_KEY)).toBeNull();
  });

  test("is handed over once, so it cannot follow the next recording", () => {
    const storage = fakeStorage();
    writeMeetingRoster(storage, ["Ксюша"]);

    expect(takeMeetingRoster(storage)).toEqual(["Ксюша"]);
    expect(takeMeetingRoster(storage)).toEqual([]);
  });

  test("reaches the backend as a roster, not as a calendar invitation", async () => {
    invocations.length = 0;
    await saveMeetingParticipants("meeting-1", ["Миша"], SOURCE_MANUAL_ROSTER);

    expect(invocations[0]!.args).toEqual({
      meetingId: "meeting-1",
      participants: ["Миша"],
      source: "manual_roster",
    });
  });
});
