import { beforeEach, describe, expect, mock, test } from "bun:test";

const invokeMock = mock(async () => null);

mock.module("@tauri-apps/api/core", () => ({
  invoke: invokeMock,
}));

function installLocalStorage() {
  const values = new Map();

  Object.defineProperty(globalThis, "window", {
    configurable: true,
    value: {
      localStorage: {
        getItem: (key) => values.get(key) ?? null,
        setItem: (key, value) => {
          values.set(key, value);
        },
        removeItem: (key) => {
          values.delete(key);
        },
        clear: () => {
          values.clear();
        },
      },
    },
  });

  return values;
}

describe("summary language local fallback", () => {
  let storageValues;

  beforeEach(() => {
    invokeMock.mockReset();
    storageValues = installLocalStorage();
  });

  test("reads summary language from local fallback when meeting has no folder", async () => {
    const prefs = await import("../../src/lib/summary-language-preferences");
    storageValues.set("summaryLanguageFallback:meeting-1", "fr");
    invokeMock.mockResolvedValueOnce({
      language: null,
      storage: "local_fallback",
    });

    await expect(prefs.readMeetingSummaryLanguage("meeting-1")).resolves.toEqual({
      language: "fr",
      storage: "local_fallback",
    });
  });

  test("saves summary language locally when command reports no folder", async () => {
    const prefs = await import("../../src/lib/summary-language-preferences");
    invokeMock.mockResolvedValueOnce({
      language: null,
      storage: "local_fallback",
    });

    await expect(
      prefs.saveMeetingSummaryLanguage("meeting-1", "es"),
    ).resolves.toEqual({
      language: "es",
      storage: "local_fallback",
    });

    expect(storageValues.get("summaryLanguageFallback:meeting-1")).toBe("es");
  });

  test("clears local fallback when Auto is saved for a folderless meeting", async () => {
    const prefs = await import("../../src/lib/summary-language-preferences");
    storageValues.set("summaryLanguageFallback:meeting-1", "de");
    invokeMock.mockResolvedValueOnce({
      language: null,
      storage: "local_fallback",
    });

    await expect(
      prefs.saveMeetingSummaryLanguage("meeting-1", null),
    ).resolves.toEqual({
      language: null,
      storage: "local_fallback",
    });

    expect(storageValues.has("summaryLanguageFallback:meeting-1")).toBe(false);
  });

  test("caches detected language locally when meeting has no folder", async () => {
    const prefs = await import("../../src/lib/summary-language-preferences");
    invokeMock.mockResolvedValueOnce({
      language: null,
      storage: "local_fallback",
    });

    await prefs.saveCachedDetectedSummaryLanguage("meeting-1", "pt");

    expect(storageValues.get("detectedSummaryLanguageFallback:meeting-1")).toBe("pt");
  });
});
