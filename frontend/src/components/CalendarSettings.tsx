'use client';

import { useCallback, useEffect, useRef, useState } from 'react';
import { toast } from 'sonner';
import { Icon } from '@/components/memento/Icon';
import { Switch } from '@/components/ui/switch';
import { useLanguage } from '@/lib/i18n';
import {
  getLocalOutlookCalendarStatus,
  getUpcomingLocalOutlookMeetings,
  isLocalOutlookCalendarEnabled,
  LocalOutlookCalendarStatus,
  LocalOutlookMeeting,
  OUTLOOK_CALENDAR_EMPTY_RETRY_INTERVAL_MS,
  OUTLOOK_CALENDAR_REFRESH_INTERVAL_MS,
  requestOutlookAccessibilityPermission,
  setLocalOutlookCalendarEnabled,
} from '@/lib/localOutlookCalendar';

function formatMeetingTime(meeting: LocalOutlookMeeting, locale: string): string {
  if (meeting.is_all_day) return new Intl.DateTimeFormat(locale, { dateStyle: 'medium' }).format(new Date(meeting.start_at));
  return new Intl.DateTimeFormat(locale, {
    weekday: 'short',
    day: 'numeric',
    month: 'short',
    hour: '2-digit',
    minute: '2-digit',
  }).format(new Date(meeting.start_at));
}

export function CalendarSettings() {
  const { t, lang } = useLanguage();
  const locale = lang === 'ru' ? 'ru-RU' : 'en-US';
  const [status, setStatus] = useState<LocalOutlookCalendarStatus | null>(null);
  const [enabled, setEnabled] = useState(false);
  const [preview, setPreview] = useState<LocalOutlookMeeting[]>([]);
  const [loading, setLoading] = useState(true);
  const [refreshing, setRefreshing] = useState(false);
  const [previewLoaded, setPreviewLoaded] = useState(false);
  const [previewError, setPreviewError] = useState<string | null>(null);
  const [lastUpdated, setLastUpdated] = useState<Date | null>(null);
  const [requestingPermission, setRequestingPermission] = useState(false);
  const refreshRequestRef = useRef<Promise<void> | null>(null);

  const refreshPreview = useCallback((force = false): Promise<void> => {
    if (refreshRequestRef.current) return refreshRequestRef.current;

    setRefreshing(true);
    setPreviewError(null);
    const request = getUpcomingLocalOutlookMeetings(7, { force })
      .then((meetings) => {
        setPreview(meetings.slice(0, 3));
        setPreviewLoaded(true);
        setLastUpdated(new Date());
      })
      .catch((error) => {
        setPreviewLoaded(true);
        setPreviewError(String(error));
        throw error;
      })
      .finally(() => {
        if (refreshRequestRef.current === request) {
          refreshRequestRef.current = null;
          setRefreshing(false);
        }
      });
    refreshRequestRef.current = request;
    return request;
  }, []);

  useEffect(() => {
    let active = true;
    const refreshStatusOnFocus = () => {
      void getLocalOutlookCalendarStatus()
        .then((nextStatus) => {
          if (active) setStatus(nextStatus);
        })
        .catch(() => {
          // The explicit refresh/enable actions surface actionable errors.
        });
    };
    Promise.all([
      getLocalOutlookCalendarStatus(),
      isLocalOutlookCalendarEnabled(),
    ])
      .then(async ([nextStatus, nextEnabled]) => {
        if (!active) return;
        setStatus(nextStatus);
        setEnabled(nextEnabled);
        if (nextEnabled && nextStatus.installed) {
          await refreshPreview().catch(() => {
            // The preview shows the error and retries automatically.
          });
        }
      })
      .catch((error) => {
        if (active) {
          toast.error(t('Could not check the local Outlook calendar'), {
            description: String(error),
          });
        }
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    window.addEventListener('focus', refreshStatusOnFocus);
    return () => {
      active = false;
      window.removeEventListener('focus', refreshStatusOnFocus);
    };
  }, [refreshPreview, t]);

  const isMacAccessibility = status?.provider === 'macos-outlook-accessibility';
  const canEnable = Boolean(
    status?.supported
      && status.installed
      && (!isMacAccessibility || status.accessibility_granted),
  );
  const calendarReady = enabled && canEnable;

  useEffect(() => {
    if (!calendarReady) return;

    const refreshWhenVisible = () => {
      if (document.visibilityState === 'visible') {
        void refreshPreview().catch(() => {
          // The preview shows the error and retries automatically.
        });
      }
    };
    const onVisibilityChange = () => {
      if (document.visibilityState === 'visible') refreshWhenVisible();
    };
    const retryDelay = previewLoaded && preview.length > 0 && !previewError
      ? OUTLOOK_CALENDAR_REFRESH_INTERVAL_MS
      : OUTLOOK_CALENDAR_EMPTY_RETRY_INTERVAL_MS;
    const interval = window.setInterval(refreshWhenVisible, retryDelay);

    window.addEventListener('focus', refreshWhenVisible);
    document.addEventListener('visibilitychange', onVisibilityChange);
    return () => {
      window.clearInterval(interval);
      window.removeEventListener('focus', refreshWhenVisible);
      document.removeEventListener('visibilitychange', onVisibilityChange);
    };
  }, [calendarReady, preview.length, previewError, previewLoaded, refreshPreview]);

  const toggleEnabled = async (next: boolean) => {
    const previous = enabled;
    setEnabled(next);
    try {
      if (next) {
        await refreshPreview(true);
        const nextStatus = await getLocalOutlookCalendarStatus();
        setStatus(nextStatus);
      } else {
        setPreview([]);
        setPreviewLoaded(false);
        setPreviewError(null);
        setLastUpdated(null);
      }
      await setLocalOutlookCalendarEnabled(next);
      toast.success(t(next ? 'Local Outlook calendar enabled' : 'Local Outlook calendar disabled'));
    } catch (error) {
      setEnabled(previous);
      toast.error(t('Could not change the local Outlook calendar setting'), {
        description: String(error),
      });
    }
  };

  if (loading) {
    return <div className="p-6 text-sm text-[var(--fg3)]">{t('Loading…')}</div>;
  }

  const statusTitle = canEnable
    ? isMacAccessibility
      ? t('Outlook Accessibility is ready')
      : t(status?.running ? 'Classic Outlook is running' : 'Classic Outlook is installed')
    : isMacAccessibility && status?.installed && !status.accessibility_granted
      ? t('Accessibility permission is required')
      : t('Local Outlook is unavailable');

  return (
    <div className="space-y-6">
      <section className="rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-canvas)] p-6">
        <div className="flex items-start justify-between gap-5">
          <div className="min-w-0">
            <div className="flex items-center gap-2">
              <Icon name="calendar" size={19} className="text-[var(--gold)]" />
              <h2 className="text-base font-semibold text-[var(--fg1)]">
                {t('Local Outlook calendar')}
              </h2>
            </div>
            <p className="mt-2 max-w-2xl text-sm leading-relaxed text-[var(--fg2)]">
              {t('Read upcoming meetings from Outlook on this computer. No Microsoft Graph, OAuth, or external calendar service is used.')}
            </p>
          </div>
          <Switch
            checked={enabled}
            disabled={!canEnable}
            onCheckedChange={toggleEnabled}
            aria-label={t('Use local Outlook calendar')}
          />
        </div>

        <div className="mt-5 rounded-xl border border-[var(--border-subtle)] bg-[var(--bg-sheet)] p-4">
          <div className="flex items-start gap-3">
            <Icon
              name={canEnable ? 'check-circle' : 'info'}
              size={18}
              className={canEnable ? 'text-[var(--success)]' : 'text-[var(--fg3)]'}
            />
            <div>
              <p className="text-sm font-medium text-[var(--fg1)]">
                {statusTitle}
              </p>
              <p className="mt-1 text-xs leading-relaxed text-[var(--fg3)]">
                {status?.detail ? t(status.detail) : t('Local Outlook is unavailable')}
              </p>
              {isMacAccessibility && status?.installed && !status.accessibility_granted && (
                <button
                  type="button"
                  disabled={requestingPermission}
                  onClick={async () => {
                    setRequestingPermission(true);
                    try {
                      const nextStatus = await requestOutlookAccessibilityPermission();
                      setStatus(nextStatus);
                      toast.info(t('Allow Memento in Accessibility settings, then return and refresh.'));
                    } catch (error) {
                      toast.error(t('Could not request Accessibility permission'), {
                        description: String(error),
                      });
                    } finally {
                      setRequestingPermission(false);
                    }
                  }}
                  className="mm-button mm-button-secondary mt-3 px-3 py-2 text-xs disabled:opacity-50"
                >
                  {t(requestingPermission ? 'Opening settings…' : 'Allow Accessibility')}
                </button>
              )}
            </div>
          </div>
        </div>

        {isMacAccessibility && (
          <div className="mt-4 rounded-xl border border-[var(--border-subtle)] bg-[var(--bg-sheet)] p-4 text-xs leading-relaxed text-[var(--fg3)]">
            {t('When you refresh, Memento opens Outlook Calendar, switches to today and attempts to select Week view. It reads only the visible accessibility labels.')}
          </div>
        )}

        <div className="mt-4 flex items-start gap-2 text-xs leading-relaxed text-[var(--fg3)]">
          <Icon name="lock" size={15} className="mt-0.5 shrink-0" />
          <div className="space-y-2">
            <p>
              {t('Memento reads only the meeting title, time, calendar, and visible location. Bodies, attachments, participant addresses, and credentials are not read. Events stay in memory and are not saved to the Memento database.')}
            </p>
            <p>
              {t('Experimental compatibility mode. Your organization may block local Outlook automation. Memento reports the error and never attempts to bypass corporate security policy.')}
            </p>
          </div>
        </div>
      </section>

      {enabled && canEnable && (
        <section className="rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-canvas)] p-6">
          <div className="flex items-center justify-between gap-4">
            <div>
              <h3 className="text-sm font-semibold text-[var(--fg1)]">{t('Next meetings')}</h3>
              <p className="mt-1 text-xs text-[var(--fg3)]">
                {t('Updates automatically while Memento is open')}
                {lastUpdated
                  ? ` · ${t('Updated')} ${new Intl.DateTimeFormat(locale, {
                    hour: '2-digit',
                    minute: '2-digit',
                  }).format(lastUpdated)}`
                  : ''}
              </p>
            </div>
            <button
              type="button"
              onClick={() => {
                void refreshPreview(true).catch((error) => {
                  toast.error(t('Could not read the local Outlook calendar'), {
                    description: String(error),
                  });
                });
              }}
              disabled={refreshing}
              className="mm-button mm-button-secondary inline-flex items-center gap-2 px-3 py-2 text-xs disabled:opacity-50"
            >
              <Icon name="refresh" size={15} className={refreshing ? 'animate-spin' : ''} />
              {t(refreshing ? 'Refreshing…' : 'Refresh')}
            </button>
          </div>

          <div className="mt-4 space-y-2" aria-live="polite">
            {refreshing && !previewLoaded ? (
              <p className="rounded-xl border border-dashed border-[var(--border-subtle)] p-4 text-sm text-[var(--fg3)]">
                {t('Checking Outlook for meetings…')}
              </p>
            ) : previewError ? (
              <div className="rounded-xl border border-dashed border-[var(--border-subtle)] p-4 text-sm text-[var(--fg3)]">
                <p>{t('Could not refresh Outlook. Memento will retry automatically.')}</p>
                <p className="mt-1 break-words text-xs">{previewError}</p>
              </div>
            ) : preview.length === 0 ? (
              <p className="rounded-xl border border-dashed border-[var(--border-subtle)] p-4 text-sm text-[var(--fg3)]">
                {t('No upcoming meetings yet. Memento will check again automatically.')}
              </p>
            ) : (
              preview.map((meeting) => (
                <div key={meeting.id} className="rounded-xl border border-[var(--border-subtle)] bg-[var(--bg-sheet)] p-4">
                  <p className="truncate text-sm font-medium text-[var(--fg1)]">{meeting.subject}</p>
                  <p className="mt-1 text-xs text-[var(--fg3)]">
                    {formatMeetingTime(meeting, locale)} · {meeting.calendar_name}
                  </p>
                </div>
              ))
            )}
          </div>
        </section>
      )}
    </div>
  );
}
