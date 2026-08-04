import { describe, expect, test } from "bun:test";
import {
  AUTO_CAPTURE_MODES,
  type AutoCaptureFlags,
  type AutoCaptureMode,
  flagsForMode,
  modeFromFlags,
  modeNeedsMicrophoneSignal,
} from "../../src/lib/autoCaptureMode";

const flags = (
  auto_meeting_detection: boolean,
  auto_listening: boolean,
  background_auto_recording: boolean,
): AutoCaptureFlags => ({
  auto_meeting_detection,
  auto_listening,
  background_auto_recording,
});

describe("mode <-> flags", () => {
  test("every mode round-trips through the stored flags", () => {
    for (const mode of AUTO_CAPTURE_MODES) {
      expect(modeFromFlags(flagsForMode(mode))).toBe(mode);
    }
  });

  test("each mode writes the flag combination the detector expects", () => {
    expect(flagsForMode("off")).toEqual(flags(false, false, false));
    expect(flagsForMode("ask")).toEqual(flags(true, false, false));
    expect(flagsForMode("live")).toEqual(flags(true, true, false));
  });

  test("no mode can write the removed silent-capture flag", () => {
    // Silent background capture was removed from the product; the flag survives
    // in the API shape only so older stored settings can be migrated.
    for (const mode of AUTO_CAPTURE_MODES) {
      expect(flagsForMode(mode).background_auto_recording).toBe(false);
    }
  });

  test("switching modes never leaves a stale recording flag behind", () => {
    // Going live -> ask must clear auto_listening; otherwise the detector would
    // keep recording.
    for (const mode of AUTO_CAPTURE_MODES) {
      const written = flagsForMode(mode);
      expect(written.auto_listening && written.background_auto_recording).toBe(false);
      if (mode === "off") {
        expect(written.auto_listening).toBe(false);
        expect(written.background_auto_recording).toBe(false);
      }
    }
  });
});

describe("reading legacy and inconsistent stored flags", () => {
  test("detection off wins over any recording flag", () => {
    expect(modeFromFlags(flags(false, true, true))).toBe("off");
    expect(modeFromFlags(flags(false, false, true))).toBe("off");
  });

  test("both recording flags on reads as live, matching detector precedence", () => {
    // The detector arms auto-listening first and only falls back to background
    // capture for the calls it declines, so 'live' is the honest label.
    expect(modeFromFlags(flags(true, true, true))).toBe("live");
  });

  test("defaults from a fresh install read as live", () => {
    // NotificationSettings::default() — detection and auto-listening on, background off.
    expect(modeFromFlags(flags(true, true, false))).toBe("live");
  });

  test("detection alone is the confirmation-only mode", () => {
    expect(modeFromFlags(flags(true, false, false))).toBe("ask");
  });

  test("a legacy background-only install reads as ask", () => {
    // The backend clears background_auto_recording before detection starts, so
    // 'ask' is what this install now behaves like.
    expect(modeFromFlags(flags(true, false, true))).toBe("ask");
  });
});

describe("platform support", () => {
  test("only the live recording mode needs the microphone-session signal", () => {
    const needing = AUTO_CAPTURE_MODES.filter(modeNeedsMicrophoneSignal);
    expect(needing).toEqual(["live"] as AutoCaptureMode[]);
  });
});
