'use client';

import { type MouseEvent } from 'react';
import { Calendar, RefreshCw } from '@/components/deslop-icons';
import { Button as FluidButton } from '@/components/ui/fluid-button';
import { Switch } from '@/components/ui/switch';
import { useLocalOutlookCalendar } from '@/contexts/LocalOutlookCalendarContext';
import { useLanguage } from '@/lib/i18n';
import {
  canReadOutlookCalendar,
  type LocalOutlookMeeting,
  manualOutlookRefreshControlState,
  needsOutlookPermission,
} from '@/lib/localOutlookCalendar';
import { IconCross } from '@/vendor/deslop/primitives/material-symbols-react';

interface CalendarSettingsProps {
  variant?: 'settings' | 'home';
  onOpenMeeting?: (meeting: LocalOutlookMeeting, event: MouseEvent<HTMLButtonElement>) => void;
}

function capitalize(value: string): string {
  return value ? value.charAt(0).toLocaleUpperCase() + value.slice(1) : value;
}

const RUSSIAN_MONTHS_GENITIVE = [
  'января',
  'февраля',
  'марта',
  'апреля',
  'мая',
  'июня',
  'июля',
  'августа',
  'сентября',
  'октября',
  'ноября',
  'декабря',
] as const;

export function CalendarSettings({ variant = 'settings', onOpenMeeting }: CalendarSettingsProps) {
  const { t, lang } = useLanguage();
  const {
    status,
    enabled,
    upcomingMeetings,
    loading,
    refreshing,
    saving,
    homeCardVisible,
    setHomeCardVisible,
    toggleEnabled,
    refresh,
  } = useLocalOutlookCalendar();

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
  const refreshControl = manualOutlookRefreshControlState(refreshing, saving);

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
          const month = lang === 'ru'
            ? capitalize(RUSSIAN_MONTHS_GENITIVE[start.getMonth()])
            : capitalize(monthFormatter.format(start));

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
                  <span className="home-meeting-month">{month}</span>
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
        <div className="flex shrink-0 items-center gap-1">
          {enabled && (
            <FluidButton
              type="button"
              variant="ghost"
              size="icon-sm"
              loading={refreshControl.loading}
              disabled={refreshControl.disabled}
              onClick={() => void refresh()}
              aria-label={t('Refresh')}
              title={t('Refresh')}
            >
              <RefreshCw aria-hidden="true" size={16} />
            </FluidButton>
          )}
          <Switch
            className="shrink-0"
            checked={enabled}
            disabled={unavailable || saving}
            onCheckedChange={toggleEnabled}
            aria-label={t('Use local Outlook calendar')}
          />
        </div>
      </div>
    </section>
  );
}
