import { beforeEach, describe, expect, mock, test } from "bun:test";

/**
 * A Tauri command reads its arguments under the camelCase form of the Rust parameter
 * name, and `Option` parameters treat a missing key as `None` rather than an error.
 * So a snake_case key here is not a loud failure — it silently drops the value, which
 * is how a recording lost its calendar subject and its chosen devices and ended up
 * named after a timestamp. These tests pin the wire names.
 */
const calls: Array<{ command: string; args: Record<string, unknown> | undefined }> = [];

mock.module("@tauri-apps/api/core", () => ({
  invoke: async (command: string, args?: Record<string, unknown>) => {
    calls.push({ command, args });
    return undefined;
  },
}));

mock.module("@tauri-apps/api/event", () => ({
  listen: async () => () => {},
}));

const { recordingService } = await import("../../src/services/recordingService");

describe("startRecordingWithDevices", () => {
  beforeEach(() => {
    calls.length = 0;
  });

  test("passes the meeting name under the key the command actually reads", async () => {
    await recordingService.startRecordingWithDevices(null, null, "Weekly team sync");

    expect(calls).toHaveLength(1);
    expect(calls[0]!.command).toBe("start_recording_with_devices_and_meeting");
    expect(calls[0]!.args?.meetingName).toBe("Weekly team sync");
    expect(calls[0]!.args).not.toHaveProperty("meeting_name");
  });

  test("passes both device names under camelCase keys", async () => {
    await recordingService.startRecordingWithDevices(
      "MacBook Pro Microphone (input)",
      "BlackHole 2ch (output)",
      "Design review",
    );

    const { args } = calls[0]!;
    expect(args?.micDeviceName).toBe("MacBook Pro Microphone (input)");
    expect(args?.systemDeviceName).toBe("BlackHole 2ch (output)");
    expect(args).not.toHaveProperty("mic_device_name");
    expect(args).not.toHaveProperty("system_device_name");
  });

  test("sends no snake_case key at all", async () => {
    await recordingService.startRecordingWithDevices(null, null, "Standup");

    const keys = Object.keys(calls[0]!.args ?? {});
    expect(keys.filter((key) => key.includes("_"))).toEqual([]);
  });
});
