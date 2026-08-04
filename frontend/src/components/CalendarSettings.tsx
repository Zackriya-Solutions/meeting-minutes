'use client';

import { type MouseEvent, useCallback, useEffect, useState } from 'react';
import { toast } from 'sonner';
import { Calendar } from '@/components/deslop-icons';
import { Button as FluidButton } from '@/components/ui/fluid-button';
import { Switch } from '@/components/ui/switch';
import { useLanguage } from '@/lib/i18n';
import {
  canReadOutlookCalendar,
  getLocalOutlookCalendarStatus,
  getUpcomingLocalOutlookMeetings,
  isLocalOutlookCalendarEnabled,
  LOCAL_OUTLOOK_SETTING_CHANGED_EVENT,
  type LocalOutlookMeeting,
  LocalOutlookCalendarStatus,
  needsOutlookPermission,
  requestOutlookCalendarPermission,
  setLocalOutlookCalendarEnabled,
} from '@/lib/localOutlookCalendar';
import { IconCross } from '@/vendor/deslop/primitives/material-symbols-react';

interface CalendarSettingsProps {
  variant?: 'settings' | 'home';
  onOpenMeeting?: (meeting: LocalOutlookMeeting, event: MouseEvent<HTMLButtonElement>) => void;
}

function capitalize(value: string): string {
  return value ? value.charAt(0).toLocaleUpperCase() + value.slice(1) : value;
}

export function CalendarSettings({ variant = 'settings', onOpenMeeting }: CalendarSettingsProps) {
  const { t, lang } = useLanguage();
  const [status, setStatus] = useState<LocalOutlookCalendarStatus | null>(null);
  const [enabled, setEnabled] = useState(false);
  const [upcomingMeetings, setUpcomingMeetings] = useState<LocalOutlookMeeting[]>([]);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [homeCardVisible, setHomeCardVisible] = useState(true);

  const refreshStatus = useCallback(async () => {
    const nextStatus = await getLocalOutlookCalendarStatus();
    setStatus(nextStatus);
    return nextStatus;
  }, []);

  const loadCalendarState = useCallback(async () => {
    const [nextStatus, nextEnabled] = await Promise.all([
      getLocalOutlookCalendarStatus(),
      isLocalOutlookCalendarEnabled(),
    ]);
    const connected = nextEnabled && canReadOutlookCalendar(nextStatus);
    setStatus(nextStatus);
    setEnabled(connected);

    if (variant === 'home' && connected) {
      const meetings = await getUpcomingLocalOutlookMeetings(7);
      setUpcomingMeetings(meetings.slice(0, 2));
    } else if (variant === 'home') {
      setUpcomingMeetings([]);
    }
  }, [variant]);

  useEffect(() => {
    let active = true;

    loadCalendarState()
      .catch((error) => {
        if (!active) return;
        toast.error(t('Could not check the local Outlook calendar'), {
          description: String(error),
        });
      })
      .finally(() => {
        if (active) setLoading(false);
      });

    const handleFocus = () => {
      void loadCalendarState().catch(() => undefined);
    };
    const handleSettingChange = () => {
      void loadCalendarState().catch(() => undefined);
    };
    window.addEventListener('focus', handleFocus);
    window.addEventListener(LOCAL_OUTLOOK_SETTING_CHANGED_EVENT, handleSettingChange);
    return () => {
      active = false;
      window.removeEventListener('focus', handleFocus);
      window.removeEventListener(LOCAL_OUTLOOK_SETTING_CHANGED_EVENT, handleSettingChange);
    };
  }, [loadCalendarState, t]);

  useEffect(() => {
    if (variant !== 'home' || loading || saving || !enabled) {
      setHomeCardVisible(true);
      return;
    }

    const hideTimer = window.setTimeout(() => setHomeCardVisible(false), 1_000);
    return () => window.clearTimeout(hideTimer);
  }, [enabled, loading, saving, variant]);

  const toggleEnabled = async (next: boolean) => {
    const previous = enabled;
    setEnabled(next);
    setSaving(true);

    try {
      if (next) {
        let nextStatus = status ?? await refreshStatus();
        if (needsOutlookPermission(nextStatus)) {
          nextStatus = await requestOutlookCalendarPermission();
          setStatus(nextStatus);
        }
        if (!canReadOutlookCalendar(nextStatus)) {
          throw new Error(nextStatus.detail || t('Local Outlook is unavailable'));
        }
        const meetings = await getUpcomingLocalOutlookMeetings(7, { force: true });
        if (variant === 'home') setUpcomingMeetings(meetings.slice(0, 2));
      } else if (variant === 'home') {
        setUpcomingMeetings([]);
      }

      await setLocalOutlookCalendarEnabled(next);
      toast.success(t(next ? 'Local Outlook calendar enabled' : 'Local Outlook calendar disabled'));
    } catch (error) {
      setEnabled(previous);
      toast.error(t('Could not change the local Outlook calendar setting'), {
        description: String(error),
      });
    } finally {
      setSaving(false);
    }
  };

  if (loading && variant === 'settings') {
    return <div className="text-sm text-muted-foreground">{t('Loading…')}</div>;
  }

  const unavailable = !status?.supported || !status?.installed;
  const caption = unavailable
    ? t('Local Outlook is unavailable')
    : needsOutlookPermission(status)
      ? t('Permission to read Outlook is required')
      : t('Read upcoming meetings from Outlook on this computer. No Microsoft Graph, OAuth, or external calendar service is used.');
  const homeConnected = enabled && !loading && !saving;
  const homeTitle = homeConnected ? t('Calendar connected') : t('Connect Outlook');
  const homeCaption = homeConnected
    ? t('Meetings will appear on the home screen')
    : saving
      ? t('Connecting…')
      : unavailable
        ? t('Outlook is unavailable')
        : t('To see upcoming meetings');

  if (variant === 'home') {
    if (homeCardVisible) {
      return (
        <section className="home-meeting-card no-drag" aria-label={t('Local Outlook calendar')}>
          <article className="home-meeting-row home-outlook-card no-drag">
            <div className="home-meeting-date home-outlook-card__top">
              <Switch
                className="shrink-0"
                checked={enabled}
                disabled={loading || unavailable || saving}
                onCheckedChange={toggleEnabled}
                aria-label={t('Use local Outlook calendar')}
              />
              <FluidButton
                type="button"
                variant="ghost"
                size="icon-lg"
                className="!h-5 !w-5 text-[var(--primary-50)] [&>span:first-child]:!bg-transparent"
                onClick={() => setHomeCardVisible(false)}
                aria-label={t('Close')}
                title={t('Close')}
              >
                <IconCross aria-hidden="true" size={20} weight={400} />
              </FluidButton>
            </div>
            <div className="home-meeting-main home-outlook-card__main">
              <span className="home-meeting-copy">
                <span className="home-meeting-title">{homeTitle}</span>
                <span className="home-meeting-time">{homeCaption}</span>
              </span>
            </div>
          </article>
        </section>
      );
    }

    if (!enabled || upcomingMeetings.length === 0) return null;

    const locale = lang === 'ru' ? 'ru-RU' : 'en-US';
    const dayFormatter = new Intl.DateTimeFormat(locale, { day: 'numeric' });
    const monthFormatter = new Intl.DateTimeFormat(locale, { month: 'long' });
    const weekdayFormatter = new Intl.DateTimeFormat(locale, { weekday: 'long' });
    const timeFormatter = new Intl.DateTimeFormat(locale, { hour: '2-digit', minute: '2-digit' });

    return (
      <section className="home-meeting-card no-drag" aria-label={t('Upcoming meetings')}>
        {upcomingMeetings.map((meeting) => {
          const start = new Date(meeting.start_at);
          const end = new Date(meeting.end_at);
          const title = meeting.subject.trim() || t('Upcoming meeting');

          return (
            <button
              key={meeting.id}
              type="button"
              className="home-meeting-row no-drag"
              onClick={(event) => onOpenMeeting?.(meeting, event)}
              aria-label={`${t('Open meeting')}: ${title}`}
            >
              <span className="home-meeting-date">
                <span className="home-meeting-day home-display">{dayFormatter.format(start)}</span>
                <span className="home-meeting-date-copy">
                  <span className="home-meeting-month">{capitalize(monthFormatter.format(start))}</span>
                  <span className="home-meeting-weekday">{capitalize(weekdayFormatter.format(start))}</span>
                </span>
              </span>
              <span className="home-meeting-main">
                <span className="home-meeting-accent" aria-hidden="true" />
                <span className="home-meeting-copy">
                  <span className="home-meeting-title">{title}</span>
                  <time className="home-meeting-time" dateTime={meeting.start_at}>
                    {timeFormatter.format(start)}{'\u00a0–\u00a0'}{timeFormatter.format(end)}
                  </time>
                </span>
              </span>
            </button>
          );
        })}
      </section>
    );
  }

  return (
    <section className="settings-section settings-cell">
      <div className="settings-cell__row">
        <span className="settings-cell__avatar" aria-hidden="true">
          <Calendar size={20} />
        </span>
        <div className="settings-cell__text">
          <h3 className="settings-cell__label">{t('Local Outlook calendar')}</h3>
          <p className="settings-cell__caption">{caption}</p>
        </div>
        <Switch
          className="shrink-0"
          checked={enabled}
          disabled={unavailable || saving}
          onCheckedChange={toggleEnabled}
          aria-label={t('Use local Outlook calendar')}
        />
      </div>
    </section>
  );
}
