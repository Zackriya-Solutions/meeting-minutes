import { invoke } from '@tauri-apps/api/core';

const ENABLED_SETTING = 'calendar.local_outlook_enabled';
export const LOCAL_OUTLOOK_SETTING_CHANGED_EVENT = 'memento:local-outlook-setting-changed';
// Outlook's local automation APIs are synchronous inside Outlook. Keep background reads
// sparse; a user can still explicitly refresh, and attendee enrichment has its own cache.
const CACHE_TTL_MS = 15 * 60 * 1000;
// Calendar fixtures are useful for an isolated UI demo, but must never be enabled
// merely because the app is running in development. They otherwise replace the
// user's real Outlook entries and also become the title source for recordings.
const USE_DEV_OUTLOOK_MOCKS = process.env.NEXT_PUBLIC_USE_DEV_OUTLOOK_MOCKS === 'true';

export const OUTLOOK_CALENDAR_REFRESH_INTERVAL_MS = 15 * 60 * 1000;
export const OUTLOOK_CALENDAR_EMPTY_RETRY_INTERVAL_MS = 5 * 60 * 1000;

interface MeetingCacheEntry {
  fetchedAt: number;
  meetings: LocalOutlookMeeting[];
}

const meetingCache = new Map<string, MeetingCacheEntry>();
const inFlightRequests = new Map<string, Promise<LocalOutlookMeeting[]>>();

export type LocalOutlookPermission = 'none' | 'automation' | 'accessibility';
export type LocalOutlookPermissionState = 'granted' | 'denied' | 'undetermined' | 'unknown';

export interface LocalOutlookCalendarStatus {
  supported: boolean;
  installed: boolean;
  running: boolean;
  /** Which consent the active connector needs. */
  permission: LocalOutlookPermission;
  /** `unknown` means macOS cannot answer yet, not that access is blocked. */
  permission_state: LocalOutlookPermissionState;
  /** Only Accessibility needs an administrator; Automation never does. */
  requires_admin: boolean;
  provider: 'local-classic-outlook' | 'macos-outlook-automation' | 'macos-outlook-accessibility';
  detail: string;
}

/**
 * Whether a read can be attempted. `unknown` counts as usable: it only means
 * macOS cannot resolve the permission until Outlook runs, and the read starts
 * Outlook itself.
 */
export function canReadOutlookCalendar(status: LocalOutlookCalendarStatus): boolean {
  if (!status.supported || !status.installed) return false;
  if (status.permission === 'none') return true;
  return status.permission_state === 'granted' || status.permission_state === 'unknown';
}

/** True while the user still has to approve something for reads to succeed. */
export function needsOutlookPermission(status: LocalOutlookCalendarStatus): boolean {
  if (!status.supported || !status.installed || status.permission === 'none') return false;
  return status.permission_state === 'denied' || status.permission_state === 'undetermined';
}

/** Accessibility sees only the rendered grid and cannot expose invitee records. */
export function canReadOutlookAttendees(status: LocalOutlookCalendarStatus): boolean {
  return status.provider !== 'macos-outlook-accessibility';
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
  /**
   * Invited people as Outlook names them, organizer first. Empty when the connector
   * cannot read the invitee list (the macOS Accessibility fallback), so every caller
   * has to treat participants as optional.
   */
  attendees: string[];
}

function devOutlookStatus(): LocalOutlookCalendarStatus {
  return {
    supported: true,
    installed: true,
    running: true,
    permission: 'none',
    permission_state: 'granted',
    requires_admin: false,
    provider: 'local-classic-outlook',
    detail: 'Development Outlook calendar mock',
  };
}

function devOutlookMeetings(): LocalOutlookMeeting[] {
  const now = new Date();
  const today = new Date(now);
  today.setSeconds(0, 0);

  const relativeDate = (minutes: number) => (
    new Date(today.getTime() + minutes * 60 * 1000).toISOString()
  );

  return [
    {
      id: 'dev-outlook-design-review',
      calendar_id: 'dev-outlook-calendar',
      calendar_name: 'Календарь',
      store_name: 'Microsoft Outlook',
      subject: 'Дизайн-ревью онбординга',
      start_at: relativeDate(-10),
      end_at: relativeDate(20),
      is_all_day: false,
      is_meeting: true,
      is_recurring: false,
      location: 'Microsoft Teams',
      response_status: 'accepted',
      attendees: ['Михаил Наер', 'Аня Петрова', 'Игорь Соколов', 'Марина Волкова'],
    },
    {
      id: 'dev-outlook-product-sync',
      calendar_id: 'dev-outlook-calendar',
      calendar_name: 'Календарь',
      store_name: 'Microsoft Outlook',
      subject: 'Синк продуктовой команды',
      start_at: relativeDate(45),
      end_at: relativeDate(75),
      is_all_day: false,
      is_meeting: true,
      is_recurring: true,
      location: 'Переговорка Маяк',
      response_status: 'organized',
      attendees: ['Михаил Наер', 'Саша Орлов', 'Катя Миронова'],
    },
    {
      id: 'dev-outlook-release-planning',
      calendar_id: 'dev-outlook-calendar',
      calendar_name: 'Календарь',
      store_name: 'Microsoft Outlook',
      subject: 'Планёрка по релизу',
      start_at: relativeDate(24 * 60 + 30),
      end_at: relativeDate(24 * 60 + 75),
      is_all_day: false,
      is_meeting: true,
      is_recurring: false,
      location: 'Microsoft Teams',
      response_status: 'tentative',
      attendees: ['Михаил Наер', 'Артём Лебедев', 'Оля Макарова', 'Дима Белов'],
    },
    {
      id: 'dev-outlook-feedback-review',
      calendar_id: 'dev-outlook-calendar',
      calendar_name: 'Календарь',
      store_name: 'Microsoft Outlook',
      subject: 'Разбор обратной связи',
      start_at: relativeDate(24 * 60 + 180),
      end_at: relativeDate(24 * 60 + 240),
      is_all_day: false,
      is_meeting: true,
      is_recurring: false,
      location: null,
      response_status: 'accepted',
      attendees: ['Михаил Наер', 'Лена Крылова'],
    },
  ];
}

export async function getLocalOutlookCalendarStatus(): Promise<LocalOutlookCalendarStatus> {
  if (USE_DEV_OUTLOOK_MOCKS) return devOutlookStatus();
  return invoke<LocalOutlookCalendarStatus>('local_outlook_calendar_status');
}

export async function requestOutlookCalendarPermission(): Promise<LocalOutlookCalendarStatus> {
  if (USE_DEV_OUTLOOK_MOCKS) return devOutlookStatus();
  return invoke<LocalOutlookCalendarStatus>('request_outlook_calendar_permission');
}

export async function getUpcomingLocalOutlookMeetings(
  days = 7,
  options: { force?: boolean; includeAttendees?: boolean } = {},
): Promise<LocalOutlookMeeting[]> {
  if (USE_DEV_OUTLOOK_MOCKS) return devOutlookMeetings();

  const includeAttendees = options.includeAttendees === true;
  const cacheKey = `${days}:${includeAttendees ? 'attendees' : 'summary'}`;
  const cached = meetingCache.get(cacheKey);
  if (!options.force && cached && Date.now() - cached.fetchedAt < CACHE_TTL_MS) {
    return cached.meetings;
  }

  const existingRequest = inFlightRequests.get(cacheKey);
  if (existingRequest) return existingRequest;

  const request = invoke<LocalOutlookMeeting[]>('get_upcoming_local_outlook_meetings', {
    days,
    includeAttendees,
  })
    .then((meetings) => {
      meetingCache.set(cacheKey, {
        fetchedAt: Date.now(),
        meetings,
      });
      return meetings;
    })
    .finally(() => {
      if (inFlightRequests.get(cacheKey) === request) {
        inFlightRequests.delete(cacheKey);
      }
    });

  inFlightRequests.set(cacheKey, request);
  return request;
}

/** How early or late a calendar entry still counts as "the meeting happening now". */
export const MEETING_IN_PROGRESS_GRACE_MS = 5 * 60 * 1000;

/**
 * The timed entry covering `now`, or null. When entries overlap the one that started
 * most recently wins: that is the meeting the user just joined.
 *
 * Only entries that are meetings with other people qualify. An all-day marker, a focus
 * block, or a personal reminder shares the clock with a call without being one, and
 * naming a recording after it would be worse than not naming it at all.
 */
export function selectMeetingInProgress(
  meetings: LocalOutlookMeeting[],
  now = Date.now(),
  graceMs = MEETING_IN_PROGRESS_GRACE_MS,
): LocalOutlookMeeting | null {
  return meetings
    .filter((meeting) => {
      if (meeting.is_all_day || !meeting.is_meeting) return false;
      const start = new Date(meeting.start_at).getTime();
      const end = new Date(meeting.end_at).getTime();
      if (!Number.isFinite(start) || !Number.isFinite(end)) return false;
      return start - graceMs <= now && now <= end + graceMs;
    })
    .sort((left, right) => (
      new Date(right.start_at).getTime() - new Date(left.start_at).getTime()
    ))[0] ?? null;
}

/** One calendar entry by the id the upcoming list handed out, or null. */
export async function findLocalOutlookMeeting(id: string): Promise<LocalOutlookMeeting | null> {
  if (!await isLocalOutlookCalendarEnabled()) return null;
  const status = await getLocalOutlookCalendarStatus();
  const meetings = await getUpcomingLocalOutlookMeetings(7, {
    includeAttendees: canReadOutlookAttendees(status),
  });
  return meetings.find((meeting) => meeting.id === id) ?? null;
}

/**
 * The Outlook meeting under way right now, for naming a recording the user started
 * without picking a calendar entry.
 *
 * Never worth delaying a recording for: the answer is the fresh cache when there is
 * one, and otherwise a read that is abandoned after `timeoutMs`. Returns null on any
 * failure — an unnamed recording is a far better outcome than a late one.
 */
export async function getCurrentLocalOutlookMeeting(
  { timeoutMs = 1_500 }: { timeoutMs?: number } = {},
): Promise<LocalOutlookMeeting | null> {
  try {
    if (!await isLocalOutlookCalendarEnabled()) return null;
    const status = await getLocalOutlookCalendarStatus();
    const meetings = await Promise.race([
      // Only the current day needs enrichment here. The seven-day metadata list is for
      // browsing; walking every future invitation while the Record button waits adds
      // avoidable work inside Outlook.
      getUpcomingLocalOutlookMeetings(1, {
        includeAttendees: canReadOutlookAttendees(status),
      }),
      new Promise<null>((resolve) => { window.setTimeout(() => resolve(null), timeoutMs); }),
    ]);
    return meetings ? selectMeetingInProgress(meetings) : null;
  } catch (error) {
    console.warn('Could not check the Outlook calendar for the meeting in progress:', error);
    return null;
  }
}

export async function isLocalOutlookCalendarEnabled(): Promise<boolean> {
  if (USE_DEV_OUTLOOK_MOCKS) return true;
  const settings = await invoke<Record<string, string>>('get_app_settings');
  return settings[ENABLED_SETTING] === 'true';
}

export async function setLocalOutlookCalendarEnabled(enabled: boolean): Promise<void> {
  if (USE_DEV_OUTLOOK_MOCKS) return;
  await invoke('set_app_setting', {
    key: ENABLED_SETTING,
    value: enabled ? 'true' : 'false',
  });
  if (!enabled) meetingCache.clear();
  if (typeof window !== 'undefined') {
    window.dispatchEvent(new CustomEvent(LOCAL_OUTLOOK_SETTING_CHANGED_EVENT, {
      detail: { enabled },
    }));
  }
}
