'use client';

import { useCallback, useEffect, useState } from 'react';
import { useRouter } from 'next/navigation';
import { Icon } from '@/components/memento/Icon';
import { useLanguage } from '@/lib/i18n';
import {
  getLocalOutlookCalendarStatus,
  getUpcomingLocalOutlookMeetings,
  isLocalOutlookCalendarEnabled,
  LocalOutlookMeeting,
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
  const [error, setError] = useState<string | null>(null);
  const [startingId, setStartingId] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      const preference = await isLocalOutlookCalendarEnabled();
      setEnabled(preference);
      if (!preference) return;
      const status = await getLocalOutlookCalendarStatus();
      if (!status.installed) {
        setError(t('Outlook is not available.'));
        return;
      }
      if (status.provider === 'macos-outlook-accessibility' && !status.accessibility_granted) {
        setError(t('Accessibility permission is required'));
        return;
      }
      const events = await getUpcomingLocalOutlookMeetings(7);
      setMeetings(events.slice(0, 5));
      setError(null);
    } catch (loadError) {
      setError(String(loadError));
    } finally {
      setLoading(false);
    }
  }, [t]);

  useEffect(() => {
    void load();
    const interval = window.setInterval(() => {
      if (document.visibilityState === 'visible') void load();
    }, 5 * 60 * 1000);
    const onFocus = () => void load();
    window.addEventListener('focus', onFocus);
    return () => {
      window.clearInterval(interval);
      window.removeEventListener('focus', onFocus);
    };
  }, [load]);

  if (loading || !enabled) return null;

  return (
    <section className="mb-5 rounded-2xl border border-[var(--border-subtle)] bg-[var(--bg-sheet)] p-4 shadow-sm">
      <div className="flex items-center justify-between gap-3">
        <div className="flex min-w-0 items-center gap-2">
          <Icon name="calendar" size={17} className="shrink-0 text-[var(--gold)]" />
          <h2 className="truncate text-sm font-semibold text-[var(--fg1)]">{t('Upcoming in Outlook')}</h2>
        </div>
        <button
          type="button"
          onClick={() => void load()}
          className="mm-icon-button mm-hover h-8 w-8"
          aria-label={t('Refresh')}
        >
          <Icon name="refresh" size={15} />
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
        <p className="mt-3 text-xs text-[var(--fg3)]">{t('No meetings in the next 7 days.')}</p>
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
