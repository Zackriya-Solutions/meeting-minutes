import { beforeEach, describe, expect, mock, test } from "bun:test";

const calls: Array<{ command: string; args: Record<string, unknown> | undefined }> = [];

mock.module("@tauri-apps/api/core", () => ({
  invoke: async (command: string, args?: Record<string, unknown>) => {
    calls.push({ command, args });
    return { meeting_id: "meeting-1" };
  },
}));

const { storageService } = await import("../../src/services/storageService");

describe("saveMeeting", () => {
  beforeEach(() => {
    calls.length = 0;
  });

  test("persists Outlook invitees in the meeting-creation call", async () => {
    await storageService.saveMeeting(
      "Weekly sync",
      [],
      "/recordings/weekly-sync",
      ["Мария Петрова", "Андрей Евлампиев"],
      ["Гость"],
    );

    expect(calls).toEqual([{
      command: "api_save_transcript",
      args: {
        meetingTitle: "Weekly sync",
        transcripts: [],
        folderPath: "/recordings/weekly-sync",
        invitedParticipants: ["Мария Петрова", "Андрей Евлампиев"],
        manualRoster: ["Гость"],
      },
    }]);
  });

  test("uses the camelCase keys Tauri maps to optional Rust arguments", async () => {
    await storageService.saveMeeting("Untitled", [], null);

    const args = calls[0]!.args!;
    expect(args.invitedParticipants).toEqual([]);
    expect(args.manualRoster).toEqual([]);
    expect(args).not.toHaveProperty("invited_participants");
    expect(args).not.toHaveProperty("manual_roster");
  });
});
