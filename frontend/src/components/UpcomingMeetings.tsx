'use client';

import { useCallback, useEffect, useRef, useState } from 'react';
import { useRouter } from 'next/navigation';
import { Icon } from '@/components/memento/Icon';
import { useLanguage } from '@/lib/i18n';
import {
  getLocalOutlookCalendarStatus,
  getUpcomingLocalOutlookMeetings,
  isLocalOutlookCalendarEnabled,
  LocalOutlookMeeting,
  OUTLOOK_CALENDAR_EMPTY_RETRY_INTERVAL_MS,
  OUTLOOK_CALENDAR_REFRESH_INTERVAL_MS,
} from '@/lib/localOutlookCalendar';

interface UpcomingMeetingsProps {
  disabled: boolean;
  onStartMeeting: (title: string) => Promise<void>;
}

function meetingTimeLabel(meeting: LocalOutlookMeeting, locale: string, t: (key: string) => string): string {
  if (meeting.is_all_day) {
    return `${new Intl.DateTimeFormat(locale, {
      weekday: 'short',
      day: 'numeric',
      month: 'short',
    }).format(new Date(meeting.start_at))} · ${t('All day')}`;
  }
  return new Intl.DateTimeFormat(locale, {
    weekday: 'short',
    day: 'numeric',
    month: 'short',
    hour: '2-digit',
    minute: '2-digit',
  }).format(new Date(meeting.start_at));
}

export function UpcomingMeetings({ disabled, onStartMeeting }: UpcomingMeetingsProps) {
  const router = useRouter();
  const { t, lang } = useLanguage();
  const locale = lang === 'ru' ? 'ru-RU' : 'en-US';
  const [enabled, setEnabled] = useState(false);
  const [meetings, setMeetings] = useState<LocalOutlookMeeting[]>([]);
  const [loading, setLoading] = useState(true);
  const [refreshing, setRefreshing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [lastUpdated, setLastUpdated] = useState<Date | null>(null);
  const [startingId, setStartingId] = useState<string | null>(null);
  const loadRequestRef = useRef<Promise<void> | null>(null);

  const load = useCallback((force = false): Promise<void> => {
    if (loadRequestRef.current) return loadRequestRef.current;

    setRefreshing(true);
    const request = (async () => {
      try {
        const preference = await isLocalOutlookCalendarEnabled();
        setEnabled(preference);
        if (!preference) {
          setMeetings([]);
          setError(null);
          return;
        }
        const status = await getLocalOutlookCalendarStatus();
        if (!status.installed) {
          setError(t('Outlook is not available.'));
          return;
        }
        if (status.provider === 'macos-outlook-accessibility' && !status.accessibility_granted) {
          setError(t('Accessibility permission is required'));
          return;
        }
        const events = await getUpcomingLocalOutlookMeetings(7, { force });
        setMeetings(events.slice(0, 5));
        setLastUpdated(new Date());
        setError(null);
      } catch (loadError) {
        setError(String(loadError));
      } finally {
        setLoading(false);
      }
    })().finally(() => {
      if (loadRequestRef.current === request) {
        loadRequestRef.current = null;
        setRefreshing(false);
      }
    });
    loadRequestRef.current = request;
    return request;
  }, [t]);

  useEffect(() => {
    void load();
    const refreshWhenVisible = () => {
      if (document.visibilityState === 'visible') void load();
    };
    const onVisibilityChange = () => {
      if (document.visibilityState === 'visible') refreshWhenVisible();
    };
    const retryDelay = meetings.length > 0 && !error
      ? OUTLOOK_CALENDAR_REFRESH_INTERVAL_MS
      : OUTLOOK_CALENDAR_EMPTY_RETRY_INTERVAL_MS;
    const interval = window.setInterval(() => {
      refreshWhenVisible();
    }, retryDelay);
    window.addEventListener('focus', refreshWhenVisible);
    document.addEventListener('visibilitychange', onVisibilityChange);
    return () => {
      window.clearInterval(interval);
      window.removeEventListener('focus', refreshWhenVisible);
      document.removeEventListener('visibilitychange', onVisibilityChange);
    };
  }, [error, load, meetings.length]);

  if (loading || !enabled) return null;

  return (
    <section className="mb-5 rounded-2xl border border-[var(--border-subtle)] bg-[var(--bg-sheet)] p-4 shadow-sm">
      <div className="flex items-center justify-between gap-3">
        <div className="flex min-w-0 items-center gap-2">
          <Icon name="calendar" size={17} className="shrink-0 text-[var(--gold)]" />
          <div className="min-w-0">
            <h2 className="truncate text-sm font-semibold text-[var(--fg1)]">{t('Upcoming in Outlook')}</h2>
            <p className="truncate text-[10px] text-[var(--fg3)]">
              {refreshing
                ? t('Refreshing…')
                : lastUpdated
                  ? `${t('Updated')} ${new Intl.DateTimeFormat(locale, {
                    hour: '2-digit',
                    minute: '2-digit',
                  }).format(lastUpdated)}`
                  : t('Automatic refresh is on')}
            </p>
          </div>
        </div>
        <button
          type="button"
          disabled={refreshing}
          onClick={() => void load(true)}
          className="mm-icon-button mm-hover h-8 w-8"
          aria-label={t('Refresh')}
        >
          <Icon name="refresh" size={15} className={refreshing ? 'animate-spin' : ''} />
        </button>
      </div>

      {error ? (
        <div className="mt-3 rounded-xl border border-[var(--border-subtle)] p-3 text-xs text-[var(--fg3)]">
          <p>{error}</p>
          <button
            type="button"
            onClick={() => router.push('/settings?tab=calendar')}
            className="mt-2 font-medium text-[var(--gold)] hover:underline"
          >
            {t('Open calendar settings')}
          </button>
        </div>
      ) : meetings.length === 0 ? (
        <p className="mt-3 text-xs text-[var(--fg3)]">
          {t('No upcoming meetings yet. Memento will check again automatically.')}
        </p>
      ) : (
        <div className="mt-3 divide-y divide-[var(--border-subtle)]">
          {meetings.map((meeting) => {
            const now = Date.now();
            const inProgress = new Date(meeting.start_at).getTime() <= now
              && new Date(meeting.end_at).getTime() > now;
            return (
              <div key={meeting.id} className="flex items-center gap-3 py-3 first:pt-1 last:pb-0">
                <div className="min-w-0 flex-1">
                  <div className="flex min-w-0 items-center gap-2">
                    <p className="truncate text-sm font-medium text-[var(--fg1)]">{meeting.subject}</p>
                    {inProgress && (
                      <span className="shrink-0 rounded-full bg-[var(--gold-soft)] px-2 py-0.5 text-[10px] font-semibold text-[var(--gold)]">
                        {t('Now')}
                      </span>
                    )}
                  </div>
                  <p className="mt-0.5 truncate text-xs text-[var(--fg3)]">
                    {meetingTimeLabel(meeting, locale, t)} · {meeting.calendar_name}
                    {meeting.location ? ` · ${meeting.location}` : ''}
                  </p>
                </div>
                {!meeting.is_all_day && (
                  <button
                    type="button"
                    disabled={disabled || startingId !== null}
                    onClick={async () => {
                      setStartingId(meeting.id);
                      try {
                        await onStartMeeting(meeting.subject);
                      } finally {
                        setStartingId(null);
                      }
                    }}
                    className="mm-button mm-button-secondary shrink-0 px-3 py-1.5 text-xs disabled:opacity-50"
                  >
                    {t(startingId === meeting.id ? 'Starting…' : 'Record')}
                  </button>
                )}
              </div>
            );
          })}
        </div>
      )}
    </section>
  );
}
