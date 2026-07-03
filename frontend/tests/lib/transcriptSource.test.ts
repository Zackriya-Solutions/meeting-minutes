import { describe, expect, test } from "bun:test";
import { formatTranscriptLine, transcriptSourceLabel } from "../../src/lib/transcriptSource";

describe("transcript source labels", () => {
  test("labels confident microphone and system audio sources", () => {
    expect(transcriptSourceLabel("microphone", 0.91)).toBe("Me");
    expect(transcriptSourceLabel("system", 0.88)).toBe("Speaker");
  });

  test("omits labels when a transcript has no source metadata", () => {
    expect(transcriptSourceLabel(null, null)).toBeNull();
    expect(transcriptSourceLabel(undefined, undefined)).toBeNull();
  });

  test("labels every attributed non-confident-microphone source as Speaker", () => {
    expect(transcriptSourceLabel("mixed", 0.9)).toBe("Speaker");
    expect(transcriptSourceLabel("unknown", 0.9)).toBe("Speaker");
    expect(transcriptSourceLabel("microphone", undefined)).toBe("Speaker");
    expect(transcriptSourceLabel("microphone", 0.54)).toBe("Speaker");
    expect(transcriptSourceLabel("microphone", 1.2)).toBe("Speaker");
    expect(transcriptSourceLabel("not-valid", 0.9)).toBe("Speaker");
  });

  test("formats legacy transcript lines without fabricated source labels", () => {
    expect(
      formatTranscriptLine(
        {
          id: "legacy",
          text: "historical transcript",
          timestamp: "12:00:00",
          audio_source: null,
          source_confidence: null,
        },
        "[00:01]",
      ),
    ).toBe("[00:01] historical transcript");
  });

  test("formats transcript lines with the Speaker fallback label", () => {
    expect(
      formatTranscriptLine(
        {
          id: "1",
          text: "hello",
          timestamp: "12:00:00",
          audio_source: "microphone",
          source_confidence: null,
        },
        "[00:01]",
      ),
    ).toBe("[00:01] Speaker: hello");
  });
});
