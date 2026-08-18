'use client';

import {
  createContext,
  type ReactNode,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
} from 'react';
import { toast } from 'sonner';
import { useLanguage } from '@/lib/i18n';
import {
  canReadOutlookCalendar,
  getLocalOutlookCalendarStatus,
  getUpcomingLocalOutlookMeetings,
  isLocalOutlookCalendarEnabled,
  LOCAL_OUTLOOK_SETTING_CHANGED_EVENT,
  OUTLOOK_CALENDAR_REFRESH_INTERVAL_MS,
  type LocalOutlookMeeting,
  type LocalOutlookCalendarStatus,
  needsOutlookPermission,
  requestOutlookCalendarPermission,
  setLocalOutlookCalendarEnabled,
} from '@/lib/localOutlookCalendar';

const HOME_MEETING_LIMIT = 2;
const MEETING_END_REFRESH_PADDING_MS = 500;
const HOME_OUTLOOK_CARD_DISMISSED_KEY = 'memento:home-outlook-card-dismissed:v1';

interface LocalOutlookCalendarContextValue {
  status: LocalOutlookCalendarStatus | null;
  enabled: boolean;
  upcomingMeetings: LocalOutlookMeeting[];
  loading: boolean;
  saving: boolean;
  homeCardVisible: boolean;
  setHomeCardVisible: (visible: boolean) => void;
  toggleEnabled: (enabled: boolean) => Promise<void>;
}

const LocalOutlookCalendarContext = createContext<LocalOutlookCalendarContextValue | null>(null);

function selectCurrentMeetings(
  meetings: LocalOutlookMeeting[],
  now = Date.now(),
): LocalOutlookMeeting[] {
  return meetings
    .filter((meeting) => {
      const start = new Date(meeting.start_at).getTime();
      const end = new Date(meeting.end_at).getTime();
      return Number.isFinite(start) && Number.isFinite(end) && end > now;
    })
    .sort((left, right) => (
      new Date(left.start_at).getTime() - new Date(right.start_at).getTime()
    ))
    .slice(0, HOME_MEETING_LIMIT);
}

export function LocalOutlookCalendarProvider({ children }: { children: ReactNode }) {
  const { t } = useLanguage();
  const [status, setStatus] = useState<LocalOutlookCalendarStatus | null>(null);
  const [enabled, setEnabled] = useState(false);
  const [upcomingMeetings, setUpcomingMeetings] = useState<LocalOutlookMeeting[]>([]);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [homeCardVisible, setHomeCardVisible] = useState(false);
  const [homeCardDismissed, setHomeCardDismissed] = useState(false);
  const [homeCardPreferenceLoaded, setHomeCardPreferenceLoaded] = useState(false);

  useEffect(() => {
    try {
      setHomeCardDismissed(window.localStorage.getItem(HOME_OUTLOOK_CARD_DISMISSED_KEY) === 'true');
    } catch {
      // The in-memory state still dismisses the card for this session when storage is unavailable.
    } finally {
      setHomeCardPreferenceLoaded(true);
    }
  }, []);

  const setHomeCardVisiblePersistently = useCallback((visible: boolean) => {
    if (!visible) {
      try {
        window.localStorage.setItem(HOME_OUTLOOK_CARD_DISMISSED_KEY, 'true');
      } catch {
        // Best effort: the local state still hides the card until the app is reloaded.
      }
      setHomeCardDismissed(true);
    }
    setHomeCardVisible(visible);
  }, []);

  const refreshStatus = useCallback(async () => {
    const nextStatus = await getLocalOutlookCalendarStatus();
    setStatus(nextStatus);
    return nextStatus;
  }, []);

  const loadCalendarState = useCallback(async (force = false) => {
    const [nextStatus, nextEnabled] = await Promise.all([
      getLocalOutlookCalendarStatus(),
      isLocalOutlookCalendarEnabled(),
    ]);
    const connected = nextEnabled && canReadOutlookCalendar(nextStatus);
    setStatus(nextStatus);
    setEnabled(connected);

    if (!connected) {
      setUpcomingMeetings([]);
      return;
    }

    const meetings = await getUpcomingLocalOutlookMeetings(7, { force });
    setUpcomingMeetings(selectCurrentMeetings(meetings));
  }, []);

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

    const refresh = () => {
      void loadCalendarState(true).catch(() => undefined);
    };
    window.addEventListener('focus', refresh);
    window.addEventListener(LOCAL_OUTLOOK_SETTING_CHANGED_EVENT, refresh);
    return () => {
      active = false;
      window.removeEventListener('focus', refresh);
      window.removeEventListener(LOCAL_OUTLOOK_SETTING_CHANGED_EVENT, refresh);
    };
  }, [loadCalendarState, t]);

  useEffect(() => {
    if (!enabled) return;

    const refresh = () => {
      void loadCalendarState(true).catch(() => undefined);
    };
    const interval = window.setInterval(refresh, OUTLOOK_CALENDAR_REFRESH_INTERVAL_MS);
    return () => window.clearInterval(interval);
  }, [enabled, loadCalendarState]);

  useEffect(() => {
    if (!enabled || upcomingMeetings.length === 0) return;

    const now = Date.now();
    const nextEnd = upcomingMeetings.reduce((nearest, meeting) => {
      const end = new Date(meeting.end_at).getTime();
      if (!Number.isFinite(end) || end <= now) return nearest;
      return Math.min(nearest, end);
    }, Number.POSITIVE_INFINITY);

    if (!Number.isFinite(nextEnd)) return;

    const timer = window.setTimeout(() => {
      void loadCalendarState(true).catch(() => undefined);
    }, Math.max(MEETING_END_REFRESH_PADDING_MS, nextEnd - now + MEETING_END_REFRESH_PADDING_MS));

    return () => window.clearTimeout(timer);
  }, [enabled, loadCalendarState, upcomingMeetings]);

  useEffect(() => {
    if (loading || !homeCardPreferenceLoaded) return;

    if (!enabled) {
      setHomeCardVisible(!homeCardDismissed);
      return;
    }

    if (saving) return;

    const hideTimer = window.setTimeout(() => setHomeCardVisible(false), 1_000);
    return () => window.clearTimeout(hideTimer);
  }, [enabled, homeCardDismissed, homeCardPreferenceLoaded, loading, saving]);

  const toggleEnabled = useCallback(async (next: boolean) => {
    const previous = enabled;
    setEnabled(next);
    setSaving(true);
    if (!homeCardDismissed) setHomeCardVisible(true);

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
        setUpcomingMeetings(selectCurrentMeetings(meetings));
      } else {
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
  }, [enabled, homeCardDismissed, refreshStatus, status, t]);

  const value = useMemo<LocalOutlookCalendarContextValue>(() => ({
    status,
    enabled,
    upcomingMeetings,
    loading,
    saving,
    homeCardVisible,
    setHomeCardVisible: setHomeCardVisiblePersistently,
    toggleEnabled,
  }), [
    enabled,
    homeCardVisible,
    setHomeCardVisiblePersistently,
    loading,
    saving,
    status,
    toggleEnabled,
    upcomingMeetings,
  ]);

  return (
    <LocalOutlookCalendarContext.Provider value={value}>
      {children}
    </LocalOutlookCalendarContext.Provider>
  );
}

export function useLocalOutlookCalendar() {
  const context = useContext(LocalOutlookCalendarContext);
  if (!context) {
    throw new Error('useLocalOutlookCalendar must be used within LocalOutlookCalendarProvider');
  }
  return context;
}
