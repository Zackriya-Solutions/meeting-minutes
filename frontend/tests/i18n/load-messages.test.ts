import { describe, it, expect } from "vitest";
import { isSupportedLocale, resolveLocaleFromOs, DEFAULT_LOCALE } from "../../src/i18n/config";

describe("i18n/config", () => {
  it("isSupportedLocale returns true for known locales", () => {
    expect(isSupportedLocale("en-US")).toBe(true);
    expect(isSupportedLocale("en-GB")).toBe(true);
    expect(isSupportedLocale("zh-CN")).toBe(true);
    expect(isSupportedLocale("zh-TW")).toBe(true);
    expect(isSupportedLocale("ja-JP")).toBe(true);
    expect(isSupportedLocale("ko-KR")).toBe(true);
  });

  it("isSupportedLocale returns false for unknown locales", () => {
    expect(isSupportedLocale("fr-FR")).toBe(false);
    expect(isSupportedLocale("")).toBe(false);
  });

  it("resolveLocaleFromOs maps zh-Hans / zh-CN to zh-CN", () => {
    expect(resolveLocaleFromOs("zh-CN")).toBe("zh-CN");
    expect(resolveLocaleFromOs("zh-Hans")).toBe("zh-CN");
  });

  it("resolveLocaleFromOs maps zh-Hant / zh-TW / zh-HK / zh-MO to zh-TW", () => {
    expect(resolveLocaleFromOs("zh-TW")).toBe("zh-TW");
    expect(resolveLocaleFromOs("zh-Hant")).toBe("zh-TW");
    expect(resolveLocaleFromOs("zh-HK")).toBe("zh-TW");
    expect(resolveLocaleFromOs("zh-MO")).toBe("zh-TW");
  });

  it("resolveLocaleFromOs maps en-GB to en-GB", () => {
    expect(resolveLocaleFromOs("en-GB")).toBe("en-GB");
  });

  it("resolveLocaleFromOs maps other en-* to en-US", () => {
    expect(resolveLocaleFromOs("en-US")).toBe("en-US");
    expect(resolveLocaleFromOs("en-AU")).toBe("en-US");
    expect(resolveLocaleFromOs("en-CA")).toBe("en-US");
  });

  it("resolveLocaleFromOs maps ja-* / ja to ja-JP", () => {
    expect(resolveLocaleFromOs("ja-JP")).toBe("ja-JP");
    expect(resolveLocaleFromOs("ja")).toBe("ja-JP");
  });

  it("resolveLocaleFromOs maps ko-* / ko to ko-KR", () => {
    expect(resolveLocaleFromOs("ko-KR")).toBe("ko-KR");
    expect(resolveLocaleFromOs("ko")).toBe("ko-KR");
  });

  it("resolveLocaleFromOs falls back to DEFAULT_LOCALE", () => {
    expect(resolveLocaleFromOs("fr-FR")).toBe(DEFAULT_LOCALE);
    expect(resolveLocaleFromOs(null)).toBe(DEFAULT_LOCALE);
    expect(resolveLocaleFromOs(undefined)).toBe(DEFAULT_LOCALE);
    expect(resolveLocaleFromOs("")).toBe(DEFAULT_LOCALE);
  });
});
