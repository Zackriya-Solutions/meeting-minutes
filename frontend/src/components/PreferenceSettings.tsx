"use client"

import { useEffect, useState, useRef } from "react"
import { Switch } from "./ui/switch"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
} from "./ui/fluid-select"
import {
  createMaterialSymbol,
  IconBell,
} from '@/vendor/deslop/primitives/material-symbols-react'
import { invoke } from "@tauri-apps/api/core"
import Analytics from "@/lib/analytics"
import AnalyticsConsentSwitch from "./AnalyticsConsentSwitch"
import { useConfig, NotificationSettings } from "@/contexts/ConfigContext"
import { TELEGRAM_AUTO_SHARE_KEY } from "@/hooks/meeting-details/useTelegramShare"
import {
  type AutoCaptureMode,
  flagsForMode,
  modeFromFlags,
  modeNeedsMicrophoneSignal,
} from "@/lib/autoCaptureMode"
import { useT } from '@/lib/i18n'

/**
 * The four behaviours, ordered least to most automatic. Labels and descriptions are
 * English translation keys (see `translations.ts`).
 */
const AUTO_CAPTURE_MODE_OPTIONS: ReadonlyArray<{
  mode: AutoCaptureMode;
  label: string;
  description: string;
}> = [
  {
    mode: 'off',
    label: 'Do nothing',
    description: 'Memento does not watch for calls. Start every recording yourself.',
  },
  {
    mode: 'ask',
    label: 'Ask me',
    description: 'Offer to start recording when a call is detected. Nothing is recorded without your confirmation.',
  },
  {
    mode: 'live',
    label: 'Record with a live transcript',
    description: 'Start recording Zoom, Teams, Telemost, and SberJazz calls right away and transcribe as you go, then save about 45 seconds after the call ends. Browser and Telegram calls still ask first.',
  },
  {
    mode: 'silent',
    label: 'Record quietly in the background',
    description: 'Capture every detected call, including browser and Telegram, without opening the recorder or transcribing live. The meeting appears in your list with audio when the call ends — press Enhance to transcribe it. Calls shorter than a minute are discarded.',
  },
];

const IconCall = createMaterialSymbol('call', 'IconCall');
const IconVoice = createMaterialSymbol('record_voice_over', 'IconVoice');

export function PreferenceSettings() {
  const {
    notificationSettings,
    isLoadingPreferences,
    loadPreferences,
    updateNotificationSettings
  } = useConfig();

  const t = useT();

  const [notificationsEnabled, setNotificationsEnabled] = useState<boolean | null>(null);
  const [isInitialLoad, setIsInitialLoad] = useState(true);
  const [previousNotificationsEnabled, setPreviousNotificationsEnabled] = useState<boolean | null>(null);
  const [isLocalOnly, setIsLocalOnly] = useState<boolean | null>(null);
  const [identityAutoAssign, setIdentityAutoAssign] = useState(false);
  const [telegramAutoShare, setTelegramAutoShare] = useState(false);
  const [backgroundCapture, setBackgroundCapture] = useState<{
    supported: boolean;
    capturing: boolean;
  } | null>(null);
  const hasTrackedViewRef = useRef(false);

  // Lazy load preferences on mount (only loads if not already cached)
  useEffect(() => {
    loadPreferences();
    // Reset tracking ref on mount (every tab visit)
    hasTrackedViewRef.current = false;
  }, [loadPreferences]);

  useEffect(() => {
    Promise.all([
      invoke<{ local_only: boolean }>('get_capture_retention_policy'),
      invoke<Record<string, string>>('get_app_settings'),
      invoke<{ supported: boolean; capturing: boolean }>('get_background_capture_status'),
    ]).then(([policy, settings, capture]) => {
      setIsLocalOnly(policy.local_only);
      setIdentityAutoAssign(settings['identity.auto_assign_enabled'] === 'true');
      setTelegramAutoShare(settings[TELEGRAM_AUTO_SHARE_KEY] === 'true');
      setBackgroundCapture({ supported: capture.supported, capturing: capture.capturing });
    }).catch((error) => {
      console.warn('Failed to load local learning policy:', error);
    });
  }, []);

  // Track preferences viewed analytics on every tab visit (once per mount)
  useEffect(() => {
    if (hasTrackedViewRef.current) return;

    const trackPreferencesViewed = async () => {
      // Wait for notification settings to be available (either from cache or after loading)
      if (notificationSettings) {
        await Analytics.track('preferences_viewed', {
          notifications_enabled: notificationSettings.notification_preferences.show_recording_started ? 'true' : 'false'
        });
        hasTrackedViewRef.current = true;
      } else if (!isLoadingPreferences) {
        // If not loading and no settings available, track with default value
        await Analytics.track('preferences_viewed', {
          notifications_enabled: 'false'
        });
        hasTrackedViewRef.current = true;
      }
    };

    trackPreferencesViewed();
  }, [notificationSettings, isLoadingPreferences]);

  // Update notificationsEnabled when notificationSettings are loaded from global state
  useEffect(() => {
    if (notificationSettings) {
      // Notification enabled means both started and stopped notifications are enabled
      const enabled =
        notificationSettings.notification_preferences.show_recording_started &&
        notificationSettings.notification_preferences.show_recording_stopped;
      setNotificationsEnabled(enabled);
      if (isInitialLoad) {
        setPreviousNotificationsEnabled(enabled);
        setIsInitialLoad(false);
      }
    } else if (!isLoadingPreferences) {
      // If not loading and no settings, use default
      setNotificationsEnabled(true);
      if (isInitialLoad) {
        setPreviousNotificationsEnabled(true);
        setIsInitialLoad(false);
      }
    }
  }, [notificationSettings, isLoadingPreferences, isInitialLoad])

  useEffect(() => {
    // Skip update on initial load or if value hasn't actually changed
    if (isInitialLoad || notificationsEnabled === null || notificationsEnabled === previousNotificationsEnabled) return;
    if (!notificationSettings) return;

    const handleUpdateNotificationSettings = async () => {
      console.log("Updating notification settings to:", notificationsEnabled);

      try {
        // Update the notification preferences
        const updatedSettings: NotificationSettings = {
          ...notificationSettings,
          notification_preferences: {
            ...notificationSettings.notification_preferences,
            show_recording_started: notificationsEnabled,
            show_recording_stopped: notificationsEnabled,
          }
        };

        console.log("Calling updateNotificationSettings with:", updatedSettings);
        await updateNotificationSettings(updatedSettings);
        setPreviousNotificationsEnabled(notificationsEnabled);
        console.log("Successfully updated notification settings to:", notificationsEnabled);

        // Track notification preference change - only fires when user manually toggles
        await Analytics.track('notification_settings_changed', {
          notifications_enabled: notificationsEnabled.toString()
        });
      } catch (error) {
        console.error('Failed to update notification settings:', error);
      }
    };

    handleUpdateNotificationSettings();
  }, [notificationsEnabled, notificationSettings, isInitialLoad, previousNotificationsEnabled, updateNotificationSettings])

  // Show loading only if we're actually loading and don't have cached data
  if (isLoadingPreferences && !notificationSettings) {
    return <div className="max-w-2xl mx-auto p-6">{t('Loading Preferences...')}</div>
  }

  // Show loading if notificationsEnabled hasn't been determined yet
  if (notificationsEnabled === null && !isLoadingPreferences) {
    return <div className="max-w-2xl mx-auto p-6">{t('Loading Preferences...')}</div>
  }

  // Ensure we have a boolean value for the Switch component
  const notificationsEnabledValue = notificationsEnabled ?? false;

  const autoCaptureMode = notificationSettings ? modeFromFlags(notificationSettings) : null;

  const handleAutoCaptureModeChange = async (mode: AutoCaptureMode) => {
    if (!notificationSettings || mode === autoCaptureMode) return;
    try {
      await updateNotificationSettings({ ...notificationSettings, ...flagsForMode(mode) });
      await Analytics.track('auto_capture_mode_changed', { mode });
    } catch (error) {
      console.error('Failed to update what happens when a call is detected:', error);
    }
  };

  const handleTelegramAutoShare = async (enabled: boolean) => {
    setTelegramAutoShare(enabled);
    try {
      await invoke('set_app_setting', {
        key: TELEGRAM_AUTO_SHARE_KEY,
        value: enabled ? 'true' : 'false',
      });
    } catch (error) {
      setTelegramAutoShare(!enabled);
      console.error('Failed to save Telegram auto-share setting:', error);
    }
  };

  const handleIdentityAutoAssign = async (enabled: boolean) => {
    setIdentityAutoAssign(enabled);
    try {
      await invoke('set_app_setting', {
        key: 'identity.auto_assign_enabled',
        value: enabled ? 'true' : 'false',
      });
    } catch (error) {
      setIdentityAutoAssign(!enabled);
      console.error('Failed to save identity policy:', error);
    }
  };

  return (
    <div className="space-y-6">
      {/* Уведомления Section */}
      <div className="settings-section settings-cell">
        <div className="settings-cell__row">
          <span className="settings-cell__avatar" aria-hidden="true">
            <IconBell size={20} weight={400} />
          </span>
          <div className="settings-cell__text">
            <h3 className="settings-cell__label">{t('Notifications')}</h3>
            <p className="settings-cell__caption">{t('Enable or disable notifications of start and end of meeting')}</p>
          </div>
          <Switch className="shrink-0" checked={notificationsEnabledValue} onCheckedChange={setNotificationsEnabled} />
        </div>
      </div>

      {/* One choice instead of three toggles: detection, auto-listening and background
          capture only ever described four behaviours. Detection evidence stays in
          memory; only the normalized capture lifecycle is stored. */}
      <div className="settings-section settings-cell">
        <div className="settings-cell__row">
          <span className="settings-cell__avatar" aria-hidden="true">
            <IconCall size={20} weight={400} />
          </span>
          <div className="settings-cell__text">
            <h3 className="settings-cell__label">{t('When a call is detected')}</h3>
            <p className="settings-cell__caption">
              {backgroundCapture?.capturing
                ? t('Recording a call in the background right now.')
                : t('Choose how Memento responds when it detects a call')}
            </p>
          </div>
          <div className="settings-cell__control">
            <Select
              value={autoCaptureMode ?? ''}
              disabled={!notificationSettings}
              onValueChange={(value) => void handleAutoCaptureModeChange(value as AutoCaptureMode)}
            >
              <SelectTrigger
                className="settings-cell__select settings-cell__device-select"
                placeholder={t('Choose an action')}
              />
              <SelectContent>
                {AUTO_CAPTURE_MODE_OPTIONS.map(({ mode, label }, index) => (
                  <SelectItem
                    key={mode}
                    index={index}
                    value={mode}
                    disabled={modeNeedsMicrophoneSignal(mode) && backgroundCapture?.supported === false}
                  >
                    {t(label)}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
        </div>
      </div>

      {/* Hidden in local-only mode: handing a summary to Telegram takes it off this machine. */}
      {!isLocalOnly && (
        <div className="settings-section">
          <div className="flex items-start justify-between gap-4 sm:gap-6">
            <div className="min-w-0">
              <h3 className="text-lg font-semibold text-[var(--fg1)] mb-2">{t('Open Telegram after a summary is generated')}</h3>
              <p className="text-sm text-[var(--fg2)]">
                {t('Telegram opens its chat picker with the meeting title in the draft and the summary on the clipboard, ready to paste. You choose the chat and press send — the app never sends anything by itself.')}
              </p>
            </div>
            <Switch
              className="shrink-0"
              checked={telegramAutoShare}
              onCheckedChange={handleTelegramAutoShare}
            />
          </div>
        </div>
      )}

      <div className="settings-section settings-cell">
        <div className="settings-cell__row">
          <span className="settings-cell__avatar" aria-hidden="true">
            <IconVoice size={20} weight={400} />
          </span>
          <div className="settings-cell__text">
            <h3 className="settings-cell__label">{t('Automatic assignment for very high-confidence known voices')}</h3>
            <p className="settings-cell__caption">
              {t('Voice profiles, corrections, and capture evidence stay local. Predictions never become training examples without an explicit confirmation.')}
            </p>
          </div>
          <Switch className="shrink-0" checked={identityAutoAssign} onCheckedChange={handleIdentityAutoAssign} />
        </div>
      </div>

      {/* Analytics Section */}
      <div className="settings-section settings-cell">
        <AnalyticsConsentSwitch />
      </div>
    </div>
  )
}
