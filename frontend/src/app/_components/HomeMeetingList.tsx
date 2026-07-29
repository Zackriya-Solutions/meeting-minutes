'use client';

import { type FormEvent, type MouseEvent, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { useRouter } from 'next/navigation';
import { Icon } from '@/components/memento/Icon';
import {
  InputGroup,
  InputGroupAddon,
  InputGroupButton,
  InputGroupTextarea,
} from '@/components/ui/input-group';
import { useSidebar, type CurrentMeeting } from '@/components/Sidebar/SidebarProvider';
import { useLanguage } from '@/lib/i18n';
import { getMeetingDisplayInfo } from '@/lib/meetingDisplay';
import { Cell, CellText } from '@/vendor/deslop/mini-app/Cell';

interface CalendarMeeting {
  meeting: CurrentMeeting;
  title: string;
  date: Date | null;
  dateLabel: string;
  time: string;
}

interface UpcomingMeeting {
  id: string;
  title: string;
  date: Date;
  end: Date;
  day: string;
  month: string;
  weekday: string;
  time: string;
}

const UPCOMING_MEETING_MOCKS = [
  { id: 'weekly-sync', title: 'Weekly team sync', dayOffset: 1, hour: 18, minute: 10, duration: 30 },
  { id: 'product-review', title: 'Product review', dayOffset: 4, hour: 12, minute: 0, duration: 60 },
  { id: 'client-call', title: 'Client call', dayOffset: 7, hour: 10, minute: 30, duration: 45 },
  { id: 'design-review', title: 'Design review', dayOffset: 8, hour: 15, minute: 0, duration: 45 },
  { id: 'sprint-planning', title: 'Sprint planning', dayOffset: 11, hour: 11, minute: 0, duration: 60 },
] as const;

export const HOME_SCROLL_POSITION_KEY = 'memento:home-scroll-position';

function rememberHomeScrollPosition(target: HTMLElement) {
  const scrollContainer = target.closest<HTMLElement>('[data-home-scroll-container]');
  if (!scrollContainer) return;

  window.sessionStorage.setItem(HOME_SCROLL_POSITION_KEY, String(scrollContainer.scrollTop));
}

function meetingDate(meeting: CurrentMeeting): Date | null {
  const value = meeting.occurredAt ?? meeting.createdAt;
  if (!value) return null;
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? null : date;
}

function meetingTimestamp(meeting: CurrentMeeting): number {
  return meetingDate(meeting)?.getTime() ?? 0;
}

function capitalize(value: string): string {
  return value ? value.charAt(0).toUpperCase() + value.slice(1) : value;
}

function localCalendarDay(date: Date): number {
  return Date.UTC(date.getFullYear(), date.getMonth(), date.getDate());
}

function historyDateLabel(
  date: Date | null,
  locale: string,
  t: (key: string) => string,
): string {
  if (!date) return t('Date unknown');

  const daysAgo = Math.round(
    (localCalendarDay(new Date()) - localCalendarDay(date)) / 86_400_000,
  );

  if (daysAgo === 0) return t('Today');
  if (daysAgo === 1) return t('Yesterday');
  if (daysAgo === 2) return t('Day before yesterday');

  return new Intl.DateTimeFormat(locale, {
    day: 'numeric',
    month: 'long',
  }).format(date);
}

export function HomeMeetingList({ animateOnMount = true }: { animateOnMount?: boolean }) {
  const router = useRouter();
  const { meetings, setCurrentMeeting } = useSidebar();
  const { t, lang } = useLanguage();
  const [question, setQuestion] = useState('');
  const screenRef = useRef<HTMLDivElement>(null);
  const hasQuestion = question.trim().length > 0;
  const locale = lang === 'ru' ? 'ru-RU' : 'en-US';

  useLayoutEffect(() => {
    const scrollContainer = screenRef.current?.closest<HTMLElement>('[data-home-scroll-container]');
    const storedPosition = Number(window.sessionStorage.getItem(HOME_SCROLL_POSITION_KEY));

    if (scrollContainer && Number.isFinite(storedPosition)) {
      scrollContainer.scrollTop = Math.max(0, storedPosition);
    }
  }, []);

  const upcomingMeetings = useMemo<UpcomingMeeting[]>(() => {
    const today = new Date();
    today.setHours(0, 0, 0, 0);
    const dayFormatter = new Intl.DateTimeFormat(locale, { day: 'numeric' });
    const monthFormatter = new Intl.DateTimeFormat(locale, { day: 'numeric', month: 'long' });
    const weekdayFormatter = new Intl.DateTimeFormat(locale, { weekday: 'long' });
    const timeFormatter = new Intl.DateTimeFormat(locale, { hour: '2-digit', minute: '2-digit' });

    const now = Date.now();

    return UPCOMING_MEETING_MOCKS.map((mock) => {
      const date = new Date(today);
      date.setDate(today.getDate() + mock.dayOffset);
      date.setHours(mock.hour, mock.minute, 0, 0);
      const end = new Date(date.getTime() + mock.duration * 60_000);

      return {
        id: mock.id,
        title: t(mock.title),
        date,
        end,
        day: dayFormatter.format(date),
        month: capitalize(
          monthFormatter.formatToParts(date).find((part) => part.type === 'month')?.value ?? '',
        ),
        weekday: capitalize(weekdayFormatter.format(date)),
        time: `${timeFormatter.format(date)}\u00a0–\u00a0${timeFormatter.format(end)}`,
      };
    })
      .filter((meeting) => meeting.date.getTime() >= now)
      .sort((left, right) => left.date.getTime() - right.date.getTime())
      .slice(0, 2);
  }, [locale, t]);

  const calendarMeetings = useMemo<CalendarMeeting[]>(() => {
    return [...meetings]
      .sort((left, right) => meetingTimestamp(right) - meetingTimestamp(left))
      .map((meeting) => {
        const display = getMeetingDisplayInfo(meeting, lang);
        const date = meetingDate(meeting);
        return {
          meeting,
          title: display.title,
          date,
          dateLabel: historyDateLabel(date, locale, t),
          time: date
            ? new Intl.DateTimeFormat(locale, { hour: '2-digit', minute: '2-digit' }).format(date)
            : display.dateLabel,
        };
      });
  }, [lang, locale, meetings, t]);

  const openMeeting = (meeting: CurrentMeeting, event: MouseEvent<HTMLButtonElement>) => {
    rememberHomeScrollPosition(event.currentTarget);
    setCurrentMeeting(meeting);
    router.push(`/meeting-details?id=${encodeURIComponent(meeting.id)}`);
  };

  const openUpcomingMeeting = (meeting: UpcomingMeeting, event: MouseEvent<HTMLButtonElement>) => {
    rememberHomeScrollPosition(event.currentTarget);
    setCurrentMeeting({
      id: meeting.id,
      title: meeting.title,
      occurredAt: meeting.date.toISOString(),
    });
    const params = new URLSearchParams({
      id: meeting.id,
      upcoming: '1',
      title: meeting.title,
      start: meeting.date.toISOString(),
      end: meeting.end.toISOString(),
    });
    router.push(`/meeting-details?${params.toString()}`);
  };

  const askArchive = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const query = question.trim();
    router.push(query ? `/chat?question=${encodeURIComponent(query)}` : '/chat');
  };

  return (
    <div ref={screenRef} className={`home-screen min-h-full${animateOnMount ? '' : ' home-screen--static'}`}>
      <div className="home-screen__inner">
        <header className="home-screen__header">
          <div>
            <h1 className="home-display">{t('Upcoming meetings')}</h1>
          </div>
        </header>

        <main className="home-screen__content">
          <section className="home-meeting-card" aria-label={t('Upcoming meetings')}>
            {upcomingMeetings.map((item) => (
              <button
                key={item.id}
                type="button"
                className="home-meeting-row no-drag"
                onClick={(event) => openUpcomingMeeting(item, event)}
                aria-label={`${t('Open meeting')}: ${item.title}`}
              >
                <span className="home-meeting-date">
                  <span className="home-meeting-day home-display">{item.day}</span>
                    <span className="home-meeting-date-copy">
                      <span className="home-meeting-month">
                        {item.month}
                      </span>
                    <span className="home-meeting-weekday">{item.weekday}</span>
                  </span>
                </span>
                <span className="home-meeting-main">
                  <span className="home-meeting-accent" aria-hidden="true" />
                  <span className="home-meeting-copy">
                    <span className="home-meeting-title">{item.title}</span>
                    <time className="home-meeting-time" dateTime={item.date.toISOString()}>
                      {item.time}
                    </time>
                  </span>
                </span>
              </button>
            ))}
          </section>

          {calendarMeetings.length > 0 ? (
            <section className="home-history" aria-label={t('Earlier')}>
              <h2>{t('Earlier')}</h2>
              <div className="home-history__list">
                {calendarMeetings.map((item) => (
                  <Cell
                    key={item.meeting.id}
                    type="button"
                    className="no-drag"
                    onClick={(event) => openMeeting(item.meeting, event)}
                    start={(
                      <span className="home-history-icon">
                        <Icon name="meeting" size={16} />
                      </span>
                    )}
                    end={<time dateTime={item.date?.toISOString()}>{item.time}</time>}
                  >
                    <CellText
                      title={item.title}
                      description={item.dateLabel}
                      bold
                    />
                  </Cell>
                ))}
              </div>
            </section>
          ) : null}
        </main>
      </div>

      <div className="home-ask-backdrop" aria-hidden="true" />
      <form className="no-drag" onSubmit={askArchive}>
        <InputGroup className="home-ask min-h-[50px] items-center rounded-[24px] bg-background">
          <InputGroupTextarea
            className="home-ask__textarea h-12 min-h-12 max-h-[240px] resize-none overflow-y-auto py-3 pl-4 pr-1 text-sm leading-6"
            rows={1}
            value={question}
            onChange={(event) => {
              setQuestion(event.target.value);
              event.currentTarget.style.height = 'auto';
              event.currentTarget.style.height = `${Math.min(event.currentTarget.scrollHeight, 240)}px`;
            }}
            onKeyDown={(event) => {
              if (event.key !== 'Enter' || event.shiftKey || event.nativeEvent.isComposing) return;
              event.preventDefault();
              if (!hasQuestion) return;
              event.currentTarget.form?.requestSubmit();
            }}
            placeholder={t('Ask anything')}
            aria-label={t('Ask anything')}
          />
          <InputGroupAddon align="inline-end" className="self-end py-2 pr-[14px]">
            <div
              className={`home-ask__submit-slot${hasQuestion ? ' is-visible' : ''}`}
              aria-hidden={!hasQuestion}
            >
              <InputGroupButton
                type={hasQuestion ? 'submit' : 'button'}
                variant="default"
                size="icon-sm"
                tabIndex={hasQuestion ? undefined : -1}
                className="home-ask__send rounded-full"
                aria-label={t('Send')}
              >
                <Icon name="chevron-up" size={16} strokeWidth={2.4} />
              </InputGroupButton>
            </div>
          </InputGroupAddon>
        </InputGroup>
      </form>
    </div>
  );
}
