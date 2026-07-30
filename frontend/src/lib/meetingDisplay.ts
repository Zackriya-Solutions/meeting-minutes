import type { Lang } from './i18n';

type Translate = (key: string) => string;

export interface MeetingDisplaySource {
  title: string;
  createdAt?: string | null;
  occurredAt?: string | null;
  folderPath?: string | null;
}

interface MeetingDisplayInfo {
  title: string;
  dateLabel: string;
  dateUnknown: boolean;
}

const WEEKDAYS: Record<string, string> = {
  mon: 'Пн',
  tue: 'Вт',
  wed: 'Ср',
  thu: 'Чт',
  fri: 'Пт',
  sat: 'Сб',
  sun: 'Вс',
};

const FALLBACK_MEETING_TITLES = [
  'Планёрка',
  'Летучка',
  'Синк',
  'Созвон',
  'Совещание',
  'Встреча',
] as const;

type FallbackMeetingTitle = typeof FALLBACK_MEETING_TITLES[number];
type GrammaticalGender = 'masculine' | 'feminine' | 'neuter';
type MeetingPeriod = 'morning' | 'day' | 'evening' | 'night';

const TIMED_TITLE_GENDERS: Record<FallbackMeetingTitle, GrammaticalGender> = {
  Планёрка: 'feminine',
  Летучка: 'feminine',
  Синк: 'masculine',
  Созвон: 'masculine',
  Совещание: 'neuter',
  Встреча: 'feminine',
};

const PERIOD_ADJECTIVES: Record<MeetingPeriod, Record<GrammaticalGender, string>> = {
  morning: { masculine: 'Утренний', feminine: 'Утренняя', neuter: 'Утреннее' },
  day: { masculine: 'Дневной', feminine: 'Дневная', neuter: 'Дневное' },
  evening: { masculine: 'Вечерний', feminine: 'Вечерняя', neuter: 'Вечернее' },
  night: { masculine: 'Ночной', feminine: 'Ночная', neuter: 'Ночное' },
};

function basename(path: string | null | undefined): string {
  if (!path) return '';
  return path.split(/[\\/]/).filter(Boolean).at(-1) ?? '';
}

function sourceName(meeting: MeetingDisplaySource): string {
  return `${meeting.title} ${basename(meeting.folderPath)}`;
}

function parseUnknownDateHint(meeting: MeetingDisplaySource): { weekday?: string; time?: string } | null {
  const match = sourceName(meeting).match(
    /date-unknown_(mon|tue|wed|thu|fri|sat|sun)_(?:(\d{2})-(\d{2})|time-unknown)/i,
  );
  if (!match) return null;
  return {
    weekday: match[1].toLowerCase(),
    time: match[2] && match[3] ? `${match[2]}:${match[3]}` : undefined,
  };
}

function parseDefaultMeetingDate(title: string): Date | null {
  const iso = title.match(/^(?:Auto )?Meeting (\d{4})-(\d{2})-(\d{2})_(\d{2})-(\d{2})-(\d{2})$/i);
  if (iso) {
    return new Date(
      Number(iso[1]),
      Number(iso[2]) - 1,
      Number(iso[3]),
      Number(iso[4]),
      Number(iso[5]),
      Number(iso[6]),
    );
  }

  const legacy = title.match(/^(?:Auto )?Meeting (\d{2})_(\d{2})_(\d{2})_(\d{2})_(\d{2})_(\d{2})$/i);
  if (legacy) {
    return new Date(
      2000 + Number(legacy[3]),
      Number(legacy[2]) - 1,
      Number(legacy[1]),
      Number(legacy[4]),
      Number(legacy[5]),
      Number(legacy[6]),
    );
  }
  return null;
}

function isMachineTitle(title: string): boolean {
  return (
    /^(?:Auto )?Meeting (?:\d{4}-\d{2}-\d{2}_\d{2}-\d{2}-\d{2}|\d{2}_\d{2}_\d{2}_\d{2}_\d{2}_\d{2})$/i.test(title)
    || /^(?:date-unknown_[a-z]{3}_(?:\d{2}-\d{2}|time-unknown)|\d{4}-\d{2}-\d{2}_(?:\d{2}-\d{2}|time-unknown))_/i.test(title)
  );
}

function descriptorFromMachineTitle(title: string): string {
  return title
    .replace(/^date-unknown_[a-z]{3}_(?:\d{2}-\d{2}|time-unknown)_/i, '')
    .replace(/^\d{4}-\d{2}-\d{2}_(?:\d{2}-\d{2}|time-unknown)_/i, '')
    .replace(/__?[a-f0-9]{8}$/i, '');
}

function humanizeDescriptor(descriptor: string): string {
  const normalized = descriptor.toLowerCase();
  const known: Record<string, string> = {
    gigatool_meeting: 'Встреча по GigaTool',
    productivity_meeting: 'Встреча команды продуктивности',
    standup_assistant: 'Стендап Assistant',
    'standup_assistant-my-part': 'Стендап Assistant · моя часть',
    standup_mini: 'Мини-стендап',
    'standup_my-part': 'Стендап · моя часть',
    'standup_proactive-harness': 'Стендап Proactive Harness',
    'planning_with-andrew': 'Планирование с Андреем',
  };
  if (known[normalized]) return known[normalized];

  const words = descriptor
    .replace(/[_-]+/g, ' ')
    .replace(/\bmeeting\b/gi, 'встреча')
    .replace(/\bstandup\b/gi, 'стендап')
    .trim();
  if (!words) return '';
  return words.charAt(0).toUpperCase() + words.slice(1);
}

function resolveMeetingHour(
  meeting: MeetingDisplaySource,
  defaultMeetingDate: Date | null,
  unknownHint: { weekday?: string; time?: string } | null,
): number | null {
  const explicitDateValue = meeting.occurredAt || defaultMeetingDate;
  if (explicitDateValue) {
    const date = explicitDateValue instanceof Date
      ? explicitDateValue
      : new Date(explicitDateValue);
    if (!Number.isNaN(date.getTime())) return date.getHours();
  }

  if (unknownHint?.time) {
    const hour = Number(unknownHint.time.split(':')[0]);
    if (Number.isInteger(hour) && hour >= 0 && hour <= 23) return hour;
  }

  if (meeting.createdAt) {
    const createdAt = new Date(meeting.createdAt);
    if (!Number.isNaN(createdAt.getTime())) return createdAt.getHours();
  }

  return null;
}

function meetingPeriod(hour: number): MeetingPeriod {
  if (hour >= 5 && hour < 12) return 'morning';
  if (hour >= 12 && hour < 17) return 'day';
  if (hour >= 17 && hour < 23) return 'evening';
  return 'night';
}

function fallbackMeetingTitle(
  meeting: MeetingDisplaySource,
  defaultMeetingDate: Date | null,
  unknownHint: { weekday?: string; time?: string } | null,
): string {
  // The machine title is available both during recording and after persistence, while
  // createdAt / folderPath arrive later. Prefer one canonical value so the generated
  // name does not change from “Evening sync” to another noun after the meeting is saved.
  const seed = meeting.title.trim()
    || meeting.occurredAt
    || meeting.createdAt
    || basename(meeting.folderPath);

  if (!seed) return 'Встреча';

  let hash = 0;
  for (let index = 0; index < seed.length; index += 1) {
    hash = Math.imul(hash, 31) + seed.charCodeAt(index);
    hash |= 0;
  }

  const title = FALLBACK_MEETING_TITLES[Math.abs(hash) % FALLBACK_MEETING_TITLES.length];
  const gender = TIMED_TITLE_GENDERS[title];
  const hour = resolveMeetingHour(meeting, defaultMeetingDate, unknownHint);

  if (!gender || hour === null) return title;

  const adjective = PERIOD_ADJECTIVES[meetingPeriod(hour)][gender];
  return `${adjective} ${title.toLocaleLowerCase('ru-RU')}`;
}

function formatKnownDate(date: Date): string {
  if (Number.isNaN(date.getTime())) return 'Дата неизвестна';
  const locale = 'ru-RU';
  const now = new Date();
  const startOfToday = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime();
  const startOfDate = new Date(date.getFullYear(), date.getMonth(), date.getDate()).getTime();
  const dayDifference = Math.round((startOfDate - startOfToday) / 86_400_000);
  const time = new Intl.DateTimeFormat(locale, { hour: '2-digit', minute: '2-digit' }).format(date);

  if (dayDifference === 0) return `Сегодня, ${time}`;
  if (dayDifference === -1) return `Вчера, ${time}`;

  const calendarDate = new Intl.DateTimeFormat(locale, {
    day: 'numeric',
    month: 'short',
    year: date.getFullYear() === now.getFullYear() ? undefined : 'numeric',
  }).format(date);
  return `${calendarDate}, ${time}`;
}

function capitalize(value: string): string {
  return value ? value.charAt(0).toUpperCase() + value.slice(1) : value;
}

function localCalendarDay(date: Date): number {
  return Date.UTC(date.getFullYear(), date.getMonth(), date.getDate());
}

export function formatRelativeMeetingDate(
  value: Date | string | null | undefined,
  locale: string,
  t: Translate,
): string {
  const date = value instanceof Date ? value : value ? new Date(value) : null;
  if (!date || Number.isNaN(date.getTime())) return t('Date unknown');

  const daysAgo = Math.round(
    (localCalendarDay(new Date()) - localCalendarDay(date)) / 86_400_000,
  );

  if (daysAgo === 0) return t('Today');
  if (daysAgo === 1) return t('Yesterday');
  if (daysAgo === 2) return t('Day before yesterday');

  return capitalize(new Intl.DateTimeFormat(locale, {
    weekday: 'long',
    day: 'numeric',
    month: 'long',
  }).format(date));
}

export function getMeetingDisplayInfo(
  meeting: MeetingDisplaySource,
  _lang: Lang,
): MeetingDisplayInfo {
  const unknownHint = parseUnknownDateHint(meeting);
  const defaultMeetingDate = parseDefaultMeetingDate(meeting.title);
  const fallbackTitle = fallbackMeetingTitle(meeting, defaultMeetingDate, unknownHint);
  const displayTitle = defaultMeetingDate
    ? fallbackTitle
    : isMachineTitle(meeting.title)
      ? humanizeDescriptor(descriptorFromMachineTitle(meeting.title)) || fallbackTitle
      : meeting.title || fallbackTitle;

  if (unknownHint) {
    const weekday = unknownHint.weekday ? WEEKDAYS[unknownHint.weekday] : undefined;
    const knownParts = [weekday, unknownHint.time].filter(Boolean).join(', ');
    const unknownText = 'дата неизвестна';
    return {
      title: displayTitle,
      dateLabel: knownParts ? `${knownParts} · ${unknownText}` : unknownText,
      dateUnknown: true,
    };
  }

  const dateValue = meeting.occurredAt || defaultMeetingDate?.toISOString() || meeting.createdAt;
  return {
    title: displayTitle,
    dateLabel: dateValue
      ? formatKnownDate(new Date(dateValue))
      : 'Дата неизвестна',
    dateUnknown: !dateValue,
  };
}
