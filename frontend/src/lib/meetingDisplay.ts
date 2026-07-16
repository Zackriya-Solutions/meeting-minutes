import type { Lang } from './i18n';

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

const WEEKDAYS: Record<string, Record<Lang, string>> = {
  mon: { ru: 'Пн', en: 'Mon' },
  tue: { ru: 'Вт', en: 'Tue' },
  wed: { ru: 'Ср', en: 'Wed' },
  thu: { ru: 'Чт', en: 'Thu' },
  fri: { ru: 'Пт', en: 'Fri' },
  sat: { ru: 'Сб', en: 'Sat' },
  sun: { ru: 'Вс', en: 'Sun' },
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
  const iso = title.match(/^Meeting (\d{4})-(\d{2})-(\d{2})_(\d{2})-(\d{2})-(\d{2})$/i);
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

  const legacy = title.match(/^Meeting (\d{2})_(\d{2})_(\d{2})_(\d{2})_(\d{2})_(\d{2})$/i);
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
    /^Meeting (?:\d{4}-\d{2}-\d{2}_\d{2}-\d{2}-\d{2}|\d{2}_\d{2}_\d{2}_\d{2}_\d{2}_\d{2})$/i.test(title)
    || /^(?:date-unknown_[a-z]{3}_(?:\d{2}-\d{2}|time-unknown)|\d{4}-\d{2}-\d{2}_(?:\d{2}-\d{2}|time-unknown))_/i.test(title)
  );
}

function descriptorFromMachineTitle(title: string): string {
  return title
    .replace(/^date-unknown_[a-z]{3}_(?:\d{2}-\d{2}|time-unknown)_/i, '')
    .replace(/^\d{4}-\d{2}-\d{2}_(?:\d{2}-\d{2}|time-unknown)_/i, '')
    .replace(/__?[a-f0-9]{8}$/i, '');
}

function humanizeDescriptor(descriptor: string, lang: Lang): string {
  const normalized = descriptor.toLowerCase();
  const known: Record<string, Record<Lang, string>> = {
    gigatool_meeting: { ru: 'Встреча по GigaTool', en: 'GigaTool meeting' },
    productivity_meeting: { ru: 'Встреча команды продуктивности', en: 'Productivity team meeting' },
    standup_assistant: { ru: 'Стендап Assistant', en: 'Assistant standup' },
    'standup_assistant-my-part': { ru: 'Стендап Assistant · моя часть', en: 'Assistant standup · my part' },
    standup_mini: { ru: 'Мини-стендап', en: 'Mini standup' },
    'standup_my-part': { ru: 'Стендап · моя часть', en: 'Standup · my part' },
    'standup_proactive-harness': { ru: 'Стендап Proactive Harness', en: 'Proactive Harness standup' },
    'planning_with-andrew': { ru: 'Планирование с Андреем', en: 'Planning with Andrew' },
  };
  if (known[normalized]) return known[normalized][lang];

  const words = descriptor
    .replace(/[_-]+/g, ' ')
    .replace(/\bmeeting\b/gi, lang === 'ru' ? 'встреча' : 'meeting')
    .replace(/\bstandup\b/gi, lang === 'ru' ? 'стендап' : 'standup')
    .trim();
  if (!words) return lang === 'ru' ? 'Без названия' : 'Untitled';
  return words.charAt(0).toUpperCase() + words.slice(1);
}

function formatKnownDate(date: Date, lang: Lang): string {
  if (Number.isNaN(date.getTime())) return lang === 'ru' ? 'Дата неизвестна' : 'Date unknown';
  const locale = lang === 'ru' ? 'ru-RU' : 'en-US';
  const now = new Date();
  const startOfToday = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime();
  const startOfDate = new Date(date.getFullYear(), date.getMonth(), date.getDate()).getTime();
  const dayDifference = Math.round((startOfDate - startOfToday) / 86_400_000);
  const time = new Intl.DateTimeFormat(locale, { hour: '2-digit', minute: '2-digit' }).format(date);

  if (dayDifference === 0) return `${lang === 'ru' ? 'Сегодня' : 'Today'}, ${time}`;
  if (dayDifference === -1) return `${lang === 'ru' ? 'Вчера' : 'Yesterday'}, ${time}`;

  const calendarDate = new Intl.DateTimeFormat(locale, {
    day: 'numeric',
    month: 'short',
    year: date.getFullYear() === now.getFullYear() ? undefined : 'numeric',
  }).format(date);
  return `${calendarDate}, ${time}`;
}

export function getMeetingDisplayInfo(
  meeting: MeetingDisplaySource,
  lang: Lang,
): MeetingDisplayInfo {
  const unknownHint = parseUnknownDateHint(meeting);
  const defaultMeetingDate = parseDefaultMeetingDate(meeting.title);
  const displayTitle = defaultMeetingDate
    ? (lang === 'ru' ? 'Без названия' : 'Untitled')
    : isMachineTitle(meeting.title)
      ? humanizeDescriptor(descriptorFromMachineTitle(meeting.title), lang)
      : meeting.title || (lang === 'ru' ? 'Без названия' : 'Untitled');

  if (unknownHint) {
    const weekday = unknownHint.weekday ? WEEKDAYS[unknownHint.weekday]?.[lang] : undefined;
    const knownParts = [weekday, unknownHint.time].filter(Boolean).join(', ');
    const unknownText = lang === 'ru' ? 'дата неизвестна' : 'date unknown';
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
      ? formatKnownDate(new Date(dateValue), lang)
      : (lang === 'ru' ? 'Дата неизвестна' : 'Date unknown'),
    dateUnknown: !dateValue,
  };
}
