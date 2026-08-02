import { describe, expect, test } from "bun:test";
import { mergeTranscripts } from "../../src/lib/transcript-merge";
import type { Transcript } from "../../src/types";

function seg(
  sequence_id: number,
  text: string,
  overrides: Partial<Transcript> = {},
): Transcript {
  return {
    id: `id-${sequence_id}`,
    text,
    timestamp: "14:30:05",
    sequence_id,
    chunk_start_time: sequence_id,
    is_partial: false,
    ...overrides,
  };
}

describe("mergeTranscripts", () => {
  describe("chunk providers (Whisper/Parakeet) — unchanged append-only behavior", () => {
    test("appends segments with fresh sequence ids", () => {
      const merged = mergeTranscripts([seg(0, "one")], [seg(1, "two")]);
      expect(merged.map((t) => t.text)).toEqual(["one", "two"]);
    });

    test("orders by chunk_start_time, then sequence_id", () => {
      const merged = mergeTranscripts(
        [],
        [
          seg(2, "third", { chunk_start_time: 9 }),
          seg(0, "first", { chunk_start_time: 1 }),
          seg(1, "second", { chunk_start_time: 5 }),
        ],
      );
      expect(merged.map((t) => t.text)).toEqual(["first", "second", "third"]);
    });

    test("an identical resend is a no-op and preserves array identity", () => {
      const prev = [seg(0, "hello")];
      expect(mergeTranscripts(prev, [seg(0, "hello")])).toBe(prev);
    });

    test("empty input returns the previous array unchanged", () => {
      const prev = [seg(0, "hello")];
      expect(mergeTranscripts(prev, [])).toBe(prev);
    });
  });

  describe("streaming providers — a segment refines in place", () => {
    test("a repeated sequence_id updates text instead of appending", () => {
      let state: Transcript[] = [];
      state = mergeTranscripts(state, [seg(0, "Hello", { is_partial: true })]);
      state = mergeTranscripts(state, [seg(0, "Hello world", { is_partial: true })]);
      state = mergeTranscripts(state, [seg(0, "Hello world.", { is_partial: false })]);

      expect(state).toHaveLength(1);
      expect(state[0].text).toBe("Hello world.");
      expect(state[0].is_partial).toBe(false);
    });

    test("keeps the original id and start time while a segment grows", () => {
      const prev = [seg(0, "Hel", { id: "original", chunk_start_time: 4, is_partial: true })];
      const merged = mergeTranscripts(prev, [
        seg(0, "Hello", { id: "later", chunk_start_time: 99, is_partial: true }),
      ]);

      // Replacing these would remount the row mid-sentence and reorder the list.
      expect(merged[0].id).toBe("original");
      expect(merged[0].chunk_start_time).toBe(4);
      expect(merged[0].text).toBe("Hello");
    });

    test("carries forward refreshed confidence and timing fields", () => {
      const prev = [seg(0, "hi", { confidence: 0.1, audio_end_time: 1, duration: 1 })];
      const merged = mergeTranscripts(prev, [
        seg(0, "hi there", { confidence: 0.9, audio_end_time: 4, duration: 4 }),
      ]);

      expect(merged[0].confidence).toBe(0.9);
      expect(merged[0].audio_end_time).toBe(4);
      expect(merged[0].duration).toBe(4);
    });

    test("a finalized segment stays put while the next one streams", () => {
      let state = mergeTranscripts([], [seg(0, "One.", { is_partial: false })]);
      state = mergeTranscripts(state, [seg(1, "Two", { is_partial: true })]);
      state = mergeTranscripts(state, [seg(1, "Two words.", { is_partial: false })]);

      expect(state.map((t) => t.text)).toEqual(["One.", "Two words."]);
    });

    test("a partial-to-final flip with identical text still updates", () => {
      const prev = [seg(0, "done.", { is_partial: true })];
      const merged = mergeTranscripts(prev, [seg(0, "done.", { is_partial: false })]);
      expect(merged[0].is_partial).toBe(false);
    });
  });

  describe("transcripts without a sequence_id", () => {
    test("are carried through and appended rather than dropped", () => {
      const legacy: Transcript = {
        id: "legacy",
        text: "loaded from history",
        timestamp: "14:00:00",
      };
      const merged = mergeTranscripts([legacy], [seg(0, "live")]);
      expect(merged.map((t) => t.text)).toContain("loaded from history");
      expect(merged).toHaveLength(2);
    });
  });
});
