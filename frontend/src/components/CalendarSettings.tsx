'use client';

import { useCallback, useEffect, useState } from 'react';
import { toast } from 'sonner';
import { Calendar } from '@/components/deslop-icons';
import { Switch } from '@/components/ui/switch';
import { useLanguage } from '@/lib/i18n';
import {
  canReadOutlookCalendar,
  getLocalOutlookCalendarStatus,
  getUpcomingLocalOutlookMeetings,
  isLocalOutlookCalendarEnabled,
  LocalOutlookCalendarStatus,
  needsOutlookPermission,
  requestOutlookCalendarPermission,
  setLocalOutlookCalendarEnabled,
} from '@/lib/localOutlookCalendar';

export function CalendarSettings() {
  const { t } = useLanguage();
  const [status, setStatus] = useState<LocalOutlookCalendarStatus | null>(null);
  const [enabled, setEnabled] = useState(false);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);

  const refreshStatus = useCallback(async () => {
    const nextStatus = await getLocalOutlookCalendarStatus();
    setStatus(nextStatus);
    return nextStatus;
  }, []);

  useEffect(() => {
    let active = true;

    Promise.all([getLocalOutlookCalendarStatus(), isLocalOutlookCalendarEnabled()])
      .then(([nextStatus, nextEnabled]) => {
        if (!active) return;
        setStatus(nextStatus);
        setEnabled(nextEnabled && canReadOutlookCalendar(nextStatus));
      })
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
      void refreshStatus().catch(() => undefined);
    };
    window.addEventListener('focus', handleFocus);
    return () => {
      active = false;
      window.removeEventListener('focus', handleFocus);
    };
  }, [refreshStatus, t]);

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
        await getUpcomingLocalOutlookMeetings(7, { force: true });
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

  if (loading) {
    return <div className="text-sm text-muted-foreground">{t('Loading…')}</div>;
  }

  const unavailable = !status?.supported || !status?.installed;
  const caption = unavailable
    ? t('Local Outlook is unavailable')
    : needsOutlookPermission(status)
      ? t('Permission to read Outlook is required')
      : t('Read upcoming meetings from Outlook on this computer. No Microsoft Graph, OAuth, or external calendar service is used.');

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
