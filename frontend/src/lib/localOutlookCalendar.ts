import { invoke } from '@tauri-apps/api/core';

const ENABLED_SETTING = 'calendar.local_outlook_enabled';
const CACHE_TTL_MS = 30 * 1000;

export const OUTLOOK_CALENDAR_REFRESH_INTERVAL_MS = 5 * 60 * 1000;
export const OUTLOOK_CALENDAR_EMPTY_RETRY_INTERVAL_MS = 60 * 1000;

interface MeetingCacheEntry {
  fetchedAt: number;
  meetings: LocalOutlookMeeting[];
}

const meetingCache = new Map<number, MeetingCacheEntry>();
const inFlightRequests = new Map<number, Promise<LocalOutlookMeeting[]>>();

export interface LocalOutlookCalendarStatus {
  supported: boolean;
  installed: boolean;
  running: boolean;
  accessibility_granted: boolean;
  provider: 'local-classic-outlook' | 'macos-outlook-accessibility';
  detail: string;
}

export interface LocalOutlookMeeting {
  id: string;
  calendar_id: string;
  calendar_name: string;
  store_name: string;
  subject: string;
  start_at: string;
  end_at: string;
  is_all_day: boolean;
  is_meeting: boolean;
  is_recurring: boolean;
  location: string | null;
  response_status: 'none' | 'organized' | 'tentative' | 'accepted' | 'declined' | 'not_responded';
}

export async function getLocalOutlookCalendarStatus(): Promise<LocalOutlookCalendarStatus> {
  return invoke<LocalOutlookCalendarStatus>('local_outlook_calendar_status');
}

export async function requestOutlookAccessibilityPermission(): Promise<LocalOutlookCalendarStatus> {
  return invoke<LocalOutlookCalendarStatus>('request_outlook_accessibility_permission');
}

export async function getUpcomingLocalOutlookMeetings(
  days = 7,
  options: { force?: boolean } = {},
): Promise<LocalOutlookMeeting[]> {
  const cached = meetingCache.get(days);
  if (!options.force && cached && Date.now() - cached.fetchedAt < CACHE_TTL_MS) {
    return cached.meetings;
  }

  const existingRequest = inFlightRequests.get(days);
  if (existingRequest) return existingRequest;

  const request = invoke<LocalOutlookMeeting[]>('get_upcoming_local_outlook_meetings', { days })
    .then((meetings) => {
      meetingCache.set(days, {
        fetchedAt: Date.now(),
        meetings,
      });
      return meetings;
    })
    .finally(() => {
      if (inFlightRequests.get(days) === request) {
        inFlightRequests.delete(days);
      }
    });

  inFlightRequests.set(days, request);
  return request;
}

export async function isLocalOutlookCalendarEnabled(): Promise<boolean> {
  const settings = await invoke<Record<string, string>>('get_app_settings');
  return settings[ENABLED_SETTING] === 'true';
}

export async function setLocalOutlookCalendarEnabled(enabled: boolean): Promise<void> {
  await invoke('set_app_setting', {
    key: ENABLED_SETTING,
    value: enabled ? 'true' : 'false',
  });
  if (!enabled) meetingCache.clear();
}
