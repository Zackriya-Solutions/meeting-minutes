import { invoke } from '@tauri-apps/api/core';
import { normaliseLanguageCode } from '@/lib/summary-languages';

export const SUMMARY_LANGUAGE_RECENTS_KEY = 'summaryLanguageRecents';
export const SUMMARY_LANGUAGE_DEFAULT_KEY = 'summaryLanguageDefault';

export type SummaryLanguageDetectionReason =
  | 'detected'
  | 'tie'
  | 'low_confidence'
  | 'unsupported'
  | 'empty';

export interface SummaryLanguageDetectionResult {
  language: string | null;
  reason: SummaryLanguageDetectionReason;
}

export function readPinnedSummaryLanguageDefault(): string | null {
  if (typeof window === 'undefined') return null;
  try {
    return normaliseLanguageCode(window.localStorage.getItem(SUMMARY_LANGUAGE_DEFAULT_KEY));
  } catch {
    return null;
  }
}

export function writePinnedSummaryLanguageDefault(value: string | null): void {
  if (typeof window === 'undefined') return;
  try {
    if (value) window.localStorage.setItem(SUMMARY_LANGUAGE_DEFAULT_KEY, value);
    else window.localStorage.removeItem(SUMMARY_LANGUAGE_DEFAULT_KEY);
  } catch {
    // Preference writes are non-critical; meeting-specific persistence happens separately.
  }
}

export async function applyPinnedSummaryLanguageToMeeting(meetingId: string): Promise<string | null> {
  const pinned = readPinnedSummaryLanguageDefault();
  if (!pinned) return null;

  await invoke('api_save_meeting_summary_language', {
    meetingId,
    summaryLanguage: pinned,
  });

  return pinned;
}

export async function readCachedDetectedSummaryLanguage(meetingId: string): Promise<string | null> {
  const cached = await invoke<string | null>('api_get_meeting_detected_summary_language', {
    meetingId,
  });
  return normaliseLanguageCode(cached);
}

export async function saveCachedDetectedSummaryLanguage(
  meetingId: string,
  language: string | null
): Promise<void> {
  await invoke('api_save_meeting_detected_summary_language', {
    meetingId,
    detectedSummaryLanguage: language,
  });
}

export async function detectTranscriptSummaryLanguage(
  transcriptTexts: string[]
): Promise<SummaryLanguageDetectionResult> {
  const detection = await invoke<SummaryLanguageDetectionResult>(
    'api_detect_transcript_summary_language',
    { transcriptTexts }
  );

  return {
    language: normaliseLanguageCode(detection.language),
    reason: detection.reason,
  };
}

export async function detectAndCacheSummaryLanguage(
  meetingId: string,
  transcriptTexts: string[]
): Promise<SummaryLanguageDetectionResult> {
  const detection = await detectTranscriptSummaryLanguage(transcriptTexts);

  if (detection.language) {
    try {
      await saveCachedDetectedSummaryLanguage(meetingId, detection.language);
    } catch (error) {
      console.warn('Failed to cache detected summary language:', error);
    }
  }

  return detection;
}
