import { invoke } from '@tauri-apps/api/core';

const ENABLED_SETTING = 'calendar.local_outlook_enabled';

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

export async function getUpcomingLocalOutlookMeetings(days = 7): Promise<LocalOutlookMeeting[]> {
  return invoke<LocalOutlookMeeting[]>('get_upcoming_local_outlook_meetings', { days });
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
}
