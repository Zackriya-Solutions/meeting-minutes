import { describe, expect, test } from "bun:test";
import { refinementLabel, type RefinementStage } from "../../src/lib/refinementProgress";

/** Identity translator: asserts on the English keys the dictionary is keyed by. */
const t = (key: string) => key;

describe("refinementLabel", () => {
  test("says nothing while idle", () => {
    expect(refinementLabel(false, null, null, t)).toBeNull();
    // Even with a stale stage hanging around, an idle pass shows no chip.
    expect(refinementLabel(false, "diarizing", null, t)).toBeNull();
  });

  test("falls back to a generic label before the first stage arrives", () => {
    // A pass started before the meeting was opened: running, but no event seen yet.
    expect(refinementLabel(true, null, null, t)).toBe("Processing");
  });

  test("names every stage the Rust pass can emit", () => {
    const expected: Record<RefinementStage, string> = {
      waiting_for_model: "Waiting for the speech model",
      diarizing: "Separating voices",
      decoding: "Reading the recording",
      transcribing: "Splitting replies",
      attributing: "Labelling speakers",
      retranscribing: "Re-transcribing",
      exporting: "Saving",
    };
    for (const [stage, label] of Object.entries(expected)) {
      expect(refinementLabel(true, stage as RefinementStage, null, t)).toBe(label);
    }
  });

  test("counts spans only while transcribing", () => {
    expect(refinementLabel(true, "transcribing", { done: 42, total: 180 }, t)).toBe(
      "Splitting replies 42/180",
    );
    // Other stages have no countable unit of work, so a stray count is ignored.
    expect(refinementLabel(true, "diarizing", { done: 42, total: 180 }, t)).toBe(
      "Separating voices",
    );
  });

  test("suppresses a meaningless 0/0 counter", () => {
    // Stages with no total emit done=0/total=0; "Splitting replies 0/0" would read as broken.
    expect(refinementLabel(true, "transcribing", { done: 0, total: 0 }, t)).toBe(
      "Splitting replies",
    );
    expect(refinementLabel(true, "transcribing", null, t)).toBe("Splitting replies");
  });
});
