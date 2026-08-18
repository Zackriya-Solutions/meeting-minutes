import { beforeEach, describe, expect, mock, test } from "bun:test";

/**
 * A Tauri command reads its arguments under the camelCase form of the Rust parameter
 * name, and `Option` parameters treat a missing key as `None` rather than an error.
 * So a snake_case key here is not a loud failure — it silently drops the value, which
 * is how a recording lost its calendar subject and its chosen devices and ended up
 * named after a timestamp. These tests pin the wire names.
 */
const calls: Array<{ command: string; args: Record<string, unknown> | undefined }> = [];
let invokeResult: unknown;

mock.module("@tauri-apps/api/core", () => ({
  invoke: async (command: string, args?: Record<string, unknown>) => {
    calls.push({ command, args });
    return invokeResult;
  },
}));

mock.module("@tauri-apps/api/event", () => ({
  listen: async () => () => {},
}));

const { recordingService } = await import("../../src/services/recordingService");

describe("startRecordingWithDevices", () => {
  beforeEach(() => {
    calls.length = 0;
    invokeResult = undefined;
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

describe("stopRecording", () => {
  beforeEach(() => {
    calls.length = 0;
    invokeResult = undefined;
  });

  test("returns the structured native shutdown outcome", async () => {
    invokeResult = {
      status: "completed_with_warnings",
      message: "Recording data was finalized with a warning",
      stop_error: "Audio device did not close cleanly",
      transcription_complete: true,
    };

    const outcome = await recordingService.stopRecording("/tmp/recording.wav");

    expect(calls[0]).toEqual({
      command: "stop_recording",
      args: { args: { save_path: "/tmp/recording.wav" } },
    });
    expect(outcome.status).toBe("completed_with_warnings");
    expect(outcome.stop_error).toBe("Audio device did not close cleanly");
  });

  test("normalizes a legacy null response so meeting persistence can continue", async () => {
    invokeResult = null;

    const outcome = await recordingService.stopRecording("/tmp/recording.wav");

    expect(outcome).toEqual({
      status: "success",
      message: "Recording stopped successfully",
      stop_error: null,
      transcription_complete: true,
    });
  });

  test("preserves the distinct already-stopped result for diagnostics", async () => {
    invokeResult = {
      status: "already_stopped",
      message: "Recording was already stopped",
      stop_error: null,
      transcription_complete: true,
    };

    const outcome = await recordingService.stopRecording("/tmp/recording.wav");

    expect(outcome.status).toBe("already_stopped");
  });
});
