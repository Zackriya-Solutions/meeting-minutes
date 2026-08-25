import { getRequestConfig } from "next-intl/server";
import { DEFAULT_LOCALE, isSupportedLocale, type Locale } from "./config";
import commonEn from "../../locales/en-US/common.json";
import recordingEn from "../../locales/en-US/recording.json";
import transcriptEn from "../../locales/en-US/transcript.json";
import summaryEn from "../../locales/en-US/summary.json";
import settingsEn from "../../locales/en-US/settings.json";
import errorsEn from "../../locales/en-US/errors.json";
import sidebarEn from "../../locales/en-US/sidebar.json";
import commonEnGb from "../../locales/en-GB/common.json";
import recordingEnGb from "../../locales/en-GB/recording.json";
import transcriptEnGb from "../../locales/en-GB/transcript.json";
import summaryEnGb from "../../locales/en-GB/summary.json";
import settingsEnGb from "../../locales/en-GB/settings.json";
import errorsEnGb from "../../locales/en-GB/errors.json";
import sidebarEnGb from "../../locales/en-GB/sidebar.json";
import commonZhCn from "../../locales/zh-CN/common.json";
import recordingZhCn from "../../locales/zh-CN/recording.json";
import transcriptZhCn from "../../locales/zh-CN/transcript.json";
import summaryZhCn from "../../locales/zh-CN/summary.json";
import settingsZhCn from "../../locales/zh-CN/settings.json";
import errorsZhCn from "../../locales/zh-CN/errors.json";
import sidebarZhCn from "../../locales/zh-CN/sidebar.json";
import commonZhTw from "../../locales/zh-TW/common.json";
import recordingZhTw from "../../locales/zh-TW/recording.json";
import transcriptZhTw from "../../locales/zh-TW/transcript.json";
import summaryZhTw from "../../locales/zh-TW/summary.json";
import settingsZhTw from "../../locales/zh-TW/settings.json";
import errorsZhTw from "../../locales/zh-TW/errors.json";
import sidebarZhTw from "../../locales/zh-TW/sidebar.json";
import commonJaJp from "../../locales/ja-JP/common.json";
import recordingJaJp from "../../locales/ja-JP/recording.json";
import transcriptJaJp from "../../locales/ja-JP/transcript.json";
import summaryJaJp from "../../locales/ja-JP/summary.json";
import settingsJaJp from "../../locales/ja-JP/settings.json";
import errorsJaJp from "../../locales/ja-JP/errors.json";
import sidebarJaJp from "../../locales/ja-JP/sidebar.json";
import commonKoKr from "../../locales/ko-KR/common.json";
import recordingKoKr from "../../locales/ko-KR/recording.json";
import transcriptKoKr from "../../locales/ko-KR/transcript.json";
import summaryKoKr from "../../locales/ko-KR/summary.json";
import settingsKoKr from "../../locales/ko-KR/settings.json";
import errorsKoKr from "../../locales/ko-KR/errors.json";
import sidebarKoKr from "../../locales/ko-KR/sidebar.json";

/**
 * Read the user stored UI language via the get_ui_language Tauri command
 * (registered by PR-12). Returns DEFAULT_LOCALE if no value has been saved yet
 * (first launch) or the stored value is not a supported locale.
 */
export async function getStoredLocale(): Promise<Locale> {
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    const stored = await invoke<string | null>("get_ui_language");
    if (stored && isSupportedLocale(stored)) return stored;
    return DEFAULT_LOCALE;
  } catch (e) {
    // SSR / build / non-Tauri context: the Tauri shell is not present.
    // We log a single warning the first time this fires, then fall back to
    // DEFAULT_LOCALE so the build and unit tests can render.
    if (typeof console !== "undefined") {
      console.warn("[i18n] get_ui_language unavailable, using default locale:", String(e));
    }
    return DEFAULT_LOCALE;
  }
}

type Messages = {
  common: typeof commonEn;
  recording: typeof recordingEn;
  transcript: typeof transcriptEn;
  summary: typeof summaryEn;
  settings: typeof settingsEn;
  errors: typeof errorsEn;
  sidebar: typeof sidebarEn;
};

const MESSAGES: Record<Locale, Messages> = {
  "en-US": {
    common: commonEn,
    recording: recordingEn,
    transcript: transcriptEn,
    summary: summaryEn,
    settings: settingsEn,
    errors: errorsEn,
    sidebar: sidebarEn,
  },
  "en-GB": {
    common: commonEnGb,
    recording: recordingEnGb,
    transcript: transcriptEnGb,
    summary: summaryEnGb,
    settings: settingsEnGb,
    errors: errorsEnGb,
    sidebar: sidebarEnGb,
  },
  "zh-CN": {
    common: commonZhCn,
    recording: recordingZhCn,
    transcript: transcriptZhCn,
    summary: summaryZhCn,
    settings: settingsZhCn,
    errors: errorsZhCn,
    sidebar: sidebarZhCn,
  },
  "zh-TW": {
    common: commonZhTw,
    recording: recordingZhTw,
    transcript: transcriptZhTw,
    summary: summaryZhTw,
    settings: settingsZhTw,
    errors: errorsZhTw,
    sidebar: sidebarZhTw,
  },
  "ja-JP": {
    common: commonJaJp,
    recording: recordingJaJp,
    transcript: transcriptJaJp,
    summary: summaryJaJp,
    settings: settingsJaJp,
    errors: errorsJaJp,
    sidebar: sidebarJaJp,
  },
  "ko-KR": {
    common: commonKoKr,
    recording: recordingKoKr,
    transcript: transcriptKoKr,
    summary: summaryKoKr,
    settings: settingsKoKr,
    errors: errorsKoKr,
    sidebar: sidebarKoKr,
  },
};

export function loadMessages(locale: Locale): Messages {
  if (!isSupportedLocale(locale)) return MESSAGES[DEFAULT_LOCALE];
  return MESSAGES[locale];
}

export default getRequestConfig(async ({ locale }) => {
  const safeLocale: Locale = locale && isSupportedLocale(locale) ? locale : DEFAULT_LOCALE;
  return {
    locale: safeLocale,
    messages: loadMessages(safeLocale),
  };
});
