import { describe, expect, test } from "bun:test";
import { buildArchivePromptSuggestions } from "../../src/lib/promptSuggestions";

const archive = [
  { title: "Планирование релиза 0.5", description: "Договорились перенести миграцию базы" },
  { title: "Ретро спринта 14", description: "Обсудили просроченные задачи по импорту аудио" },
  { title: "Meeting 3", description: null },
];

describe("archive prompt suggestions", () => {
  test("draws on the archive it was given", () => {
    // Regression guard: this function once ignored `sources` entirely and returned a
    // hard-coded list, silently dropping every content-aware suggestion.
    const ru = buildArchivePromptSuggestions(archive, "ru");
    const en = buildArchivePromptSuggestions(archive, "en");

    expect(ru.some((s) => s.includes("перенести миграцию"))).toBe(true);
    expect(en.some((s) => s.includes("перенести миграцию"))).toBe(true);
    expect(ru.some((s) => s.includes("Планирование релиза"))).toBe(true);
  });

  test("generic titles never become a suggestion", () => {
    const suggestions = buildArchivePromptSuggestions(
      [{ title: "Meeting 3", description: null }],
      "en",
    );
    expect(suggestions.some((s) => s.includes("Meeting 3"))).toBe(false);
  });

  test("an empty archive still yields usable generic prompts", () => {
    for (const lang of ["ru", "en"] as const) {
      const suggestions = buildArchivePromptSuggestions([], lang);
      expect(suggestions.length).toBeGreaterThan(0);
      expect(suggestions.every((s) => s.trim().length > 0)).toBe(true);
    }
  });

  test("suggestions are unique and stay within the rotation limit", () => {
    for (const lang of ["ru", "en"] as const) {
      const suggestions = buildArchivePromptSuggestions(archive, lang);
      expect(suggestions.length).toBeLessThanOrEqual(6);
      expect(new Set(suggestions).size).toBe(suggestions.length);
    }
  });

  test("nothing suggests ranking individual people", () => {
    // The retrieval layer can answer such a question with an explicit hedge, but the
    // archive screen should not open by proposing it.
    const personRanking = [/\bкто\b/i, /\bwho\b/i, /перформер/i, /performer/i];
    for (const lang of ["ru", "en"] as const) {
      for (const suggestion of buildArchivePromptSuggestions(archive, lang)) {
        expect(personRanking.some((p) => p.test(suggestion))).toBe(false);
      }
    }
  });
});
