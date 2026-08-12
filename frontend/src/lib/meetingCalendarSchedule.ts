export interface MeetingCalendarSchedule {
  startAt: string;
  endAt: string;
}

const PENDING_SCHEDULE_KEY = 'pendingMeetingCalendarSchedule';
const SAVED_SCHEDULES_KEY = 'memento:meeting-calendar-schedules';

function validSchedule(value: unknown): value is MeetingCalendarSchedule {
  if (!value || typeof value !== 'object') return false;
  const schedule = value as Partial<MeetingCalendarSchedule>;
  const start = new Date(schedule.startAt ?? '').getTime();
  const end = new Date(schedule.endAt ?? '').getTime();
  return Number.isFinite(start) && Number.isFinite(end) && end > start;
}

export function rememberPendingMeetingSchedule(
  storage: Storage,
  schedule?: MeetingCalendarSchedule | null,
): void {
  try {
    if (validSchedule(schedule)) {
      storage.setItem(PENDING_SCHEDULE_KEY, JSON.stringify(schedule));
    } else {
      storage.removeItem(PENDING_SCHEDULE_KEY);
    }
  } catch {
    // Calendar metadata must never prevent the recording itself from starting.
  }
}

export function takePendingMeetingSchedule(storage: Storage): MeetingCalendarSchedule | null {
  try {
    const stored = storage.getItem(PENDING_SCHEDULE_KEY);
    storage.removeItem(PENDING_SCHEDULE_KEY);
    if (!stored) return null;
    const parsed: unknown = JSON.parse(stored);
    return validSchedule(parsed) ? parsed : null;
  } catch {
    return null;
  }
}

export function saveMeetingCalendarSchedule(
  storage: Storage,
  meetingId: string,
  schedule: MeetingCalendarSchedule,
): void {
  if (!validSchedule(schedule)) return;
  try {
    let schedules: Record<string, MeetingCalendarSchedule> = {};
    const parsed: unknown = JSON.parse(storage.getItem(SAVED_SCHEDULES_KEY) ?? '{}');
    if (parsed && typeof parsed === 'object' && !Array.isArray(parsed)) {
      schedules = parsed as Record<string, MeetingCalendarSchedule>;
    }
    schedules[meetingId] = schedule;
    storage.setItem(SAVED_SCHEDULES_KEY, JSON.stringify(schedules));
  } catch {
    // Calendar metadata is optional; a storage failure must not turn a saved meeting
    // into an apparent save error.
  }
}

export function readMeetingCalendarSchedule(
  storage: Storage,
  meetingId: string,
): MeetingCalendarSchedule | null {
  try {
    const parsed: unknown = JSON.parse(storage.getItem(SAVED_SCHEDULES_KEY) ?? '{}');
    if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) return null;
    const schedule = (parsed as Record<string, unknown>)[meetingId];
    return validSchedule(schedule) ? schedule : null;
  } catch {
    return null;
  }
}
