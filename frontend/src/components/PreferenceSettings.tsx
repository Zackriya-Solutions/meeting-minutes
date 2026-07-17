"use client"

import { useEffect, useState, useRef } from "react"
import { Switch } from "./ui/switch"
import { FolderOpen } from '@/components/memento/LucideCompat'
import { invoke } from "@tauri-apps/api/core"
import Analytics from "@/lib/analytics"
import AnalyticsConsentSwitch from "./AnalyticsConsentSwitch"
import { useConfig, NotificationSettings } from "@/contexts/ConfigContext"
import { useT } from '@/lib/i18n'

export function PreferenceSettings() {
  const {
    notificationSettings,
    storageLocations,
    isLoadingPreferences,
    loadPreferences,
    updateNotificationSettings
  } = useConfig();

  const t = useT();

  const [notificationsEnabled, setNotificationsEnabled] = useState<boolean | null>(null);
  const [isInitialLoad, setIsInitialLoad] = useState(true);
  const [previousNotificationsEnabled, setPreviousNotificationsEnabled] = useState<boolean | null>(null);
  const [capturePolicy, setCapturePolicy] = useState<{
    unpromoted_retention_minutes: number;
    saved_audio_retention_days?: number | null;
    local_only: boolean;
  } | null>(null);
  const [identityAutoAssign, setIdentityAutoAssign] = useState(false);
  const [isSavingLearningPolicy, setIsSavingLearningPolicy] = useState(false);
  const hasTrackedViewRef = useRef(false);

  // Lazy load preferences on mount (only loads if not already cached)
  useEffect(() => {
    loadPreferences();
    // Reset tracking ref on mount (every tab visit)
    hasTrackedViewRef.current = false;
  }, [loadPreferences]);

  useEffect(() => {
    Promise.all([
      invoke<{
        unpromoted_retention_minutes: number;
        saved_audio_retention_days?: number | null;
        local_only: boolean;
      }>('get_capture_retention_policy'),
      invoke<Record<string, string>>('get_app_settings'),
    ]).then(([policy, settings]) => {
      setCapturePolicy(policy);
      setIdentityAutoAssign(settings['identity.auto_assign_enabled'] === 'true');
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

  const handleOpenFolder = async (folderType: 'database' | 'models' | 'recordings') => {
    try {
      switch (folderType) {
        case 'database':
          await invoke('open_database_folder');
          break;
        case 'models':
          await invoke('open_models_folder');
          break;
        case 'recordings':
          await invoke('open_recordings_folder');
          break;
      }

      // Track storage folder access
      await Analytics.track('storage_folder_opened', {
        folder_type: folderType
      });
    } catch (error) {
      console.error(`Failed to open ${folderType} folder:`, error);
    }
  };

  // Show loading only if we're actually loading and don't have cached data
  if (isLoadingPreferences && !notificationSettings && !storageLocations) {
    return <div className="max-w-2xl mx-auto p-6">{t('Loading Preferences...')}</div>
  }

  // Show loading if notificationsEnabled hasn't been determined yet
  if (notificationsEnabled === null && !isLoadingPreferences) {
    return <div className="max-w-2xl mx-auto p-6">{t('Loading Preferences...')}</div>
  }

  // Ensure we have a boolean value for the Switch component
  const notificationsEnabledValue = notificationsEnabled ?? false;

  const handleAutoMeetingDetectionChange = async (enabled: boolean) => {
    if (!notificationSettings) return;
    try {
      await updateNotificationSettings({
        ...notificationSettings,
        auto_meeting_detection: enabled,
      });
      await Analytics.track('auto_meeting_detection_setting_changed', {
        enabled: enabled.toString(),
      });
    } catch (error) {
      console.error('Failed to update automatic meeting detection:', error);
    }
  };

  const handleAutoListeningChange = async (enabled: boolean) => {
    if (!notificationSettings) return;
    try {
      await updateNotificationSettings({
        ...notificationSettings,
        auto_listening: enabled,
      });
      await Analytics.track('auto_listening_setting_changed', {
        enabled: enabled.toString(),
      });
    } catch (error) {
      console.error('Failed to update auto-listening:', error);
    }
  };

  const saveLearningPolicy = async () => {
    if (!capturePolicy) return;
    setIsSavingLearningPolicy(true);
    try {
      await invoke('update_capture_retention_policy', {
        input: {
          unpromotedRetentionMinutes: capturePolicy.unpromoted_retention_minutes,
          savedAudioRetentionDays: capturePolicy.saved_audio_retention_days ?? null,
        },
      });
    } catch (error) {
      console.error('Failed to save capture retention policy:', error);
    } finally {
      setIsSavingLearningPolicy(false);
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
      <div className="bg-[var(--bg-canvas)] rounded-lg border border-[var(--border-subtle)] p-6 shadow-none">
        <div className="flex items-start justify-between gap-4">
          <div className="min-w-0">
            <h3 className="text-lg font-semibold text-[var(--fg1)] mb-2">{t('Notifications')}</h3>
            <p className="text-sm text-[var(--fg2)]">{t('Enable or disable notifications of start and end of meeting')}</p>
          </div>
          <Switch className="shrink-0" checked={notificationsEnabledValue} onCheckedChange={setNotificationsEnabled} />
        </div>
      </div>

      {/* Detection evidence remains in memory; only normalized auto-listening lifecycle is stored. */}
      <div className="bg-[var(--bg-canvas)] rounded-lg border border-[var(--border-subtle)] p-6 shadow-none">
        <div className="flex items-start justify-between gap-4 sm:gap-6">
          <div className="min-w-0">
            <h3 className="text-lg font-semibold text-[var(--fg1)] mb-2">{t('Automatic meeting detection')}</h3>
            <p className="text-sm text-[var(--fg2)]">
              {t('Detect supported meeting apps and browser calls locally. Raw process names, window titles, and URLs are not stored.')}
            </p>
          </div>
          <Switch
            className="shrink-0"
            checked={notificationSettings?.auto_meeting_detection ?? true}
            disabled={!notificationSettings}
            onCheckedChange={handleAutoMeetingDetectionChange}
          />
        </div>
      </div>

      <div className="bg-[var(--bg-canvas)] rounded-lg border border-[var(--border-subtle)] p-6 shadow-none">
        <div className="flex items-start justify-between gap-4 sm:gap-6">
          <div className="min-w-0">
            <h3 className="text-lg font-semibold text-[var(--fg1)] mb-2">{t('Auto-listening')}</h3>
            <p className="text-sm text-[var(--fg2)]">
              {t('Start a normal local recording when a supported native meeting client begins using the microphone, then stop and save it after the signal has been absent for about 45 seconds. Browser, Telegram, and process-only signals still require confirmation.')}
            </p>
          </div>
          <Switch
            className="shrink-0"
            checked={notificationSettings?.auto_listening ?? true}
            disabled={!notificationSettings || !notificationSettings.auto_meeting_detection}
            onCheckedChange={handleAutoListeningChange}
          />
        </div>
      </div>

      <div className="bg-[var(--bg-canvas)] rounded-lg border border-[var(--border-subtle)] p-6 shadow-none">
        <h3 className="text-lg font-semibold text-[var(--fg1)] mb-2">{t('Local learning and retention')}</h3>
        <p className="text-sm text-[var(--fg2)]">
          {t('Voice profiles, corrections, and capture evidence stay local. Predictions never become training examples without an explicit confirmation.')}
        </p>
        <div className="mt-4 flex items-start justify-between gap-6 border-t border-[var(--border-subtle)] pt-4">
          <div>
            <div className="text-sm font-medium text-[var(--fg1)]">{t('Automatic assignment for very high-confidence known voices')}</div>
            <p className="mt-1 text-xs text-[var(--fg3)]">{t('Off by default. Voice floor and top-two margin still apply; uncertain voices remain Unknown.')}</p>
          </div>
          <Switch checked={identityAutoAssign} onCheckedChange={handleIdentityAutoAssign} />
        </div>
        {capturePolicy && (
          <div className="mt-4 grid gap-3 border-t border-[var(--border-subtle)] pt-4 sm:grid-cols-2">
            <label className="text-xs text-[var(--fg2)]">
              {t('Unpromoted capture metadata, minutes')}
              <input
                type="number"
                min={1}
                max={1440}
                value={capturePolicy.unpromoted_retention_minutes}
                onChange={(event) => setCapturePolicy({ ...capturePolicy, unpromoted_retention_minutes: Number(event.target.value) })}
                className="mt-1 w-full rounded-md border border-[var(--border-strong)] bg-[var(--bg-sheet)] px-3 py-2 text-sm"
              />
            </label>
            <label className="text-xs text-[var(--fg2)]">
              {t('Saved audio retention, days (blank keeps it)')}
              <input
                type="number"
                min={1}
                max={3650}
                value={capturePolicy.saved_audio_retention_days ?? ''}
                onChange={(event) => setCapturePolicy({ ...capturePolicy, saved_audio_retention_days: event.target.value ? Number(event.target.value) : null })}
                className="mt-1 w-full rounded-md border border-[var(--border-strong)] bg-[var(--bg-sheet)] px-3 py-2 text-sm"
              />
            </label>
            <div className="sm:col-span-2 flex items-center justify-between gap-3">
              <span className="text-xs text-[var(--fg3)]">{t('Local-only is enforced for capture evidence and learning profiles.')}</span>
              <button
                type="button"
                disabled={isSavingLearningPolicy}
                onClick={() => void saveLearningPolicy()}
                className="rounded-md bg-[var(--gold)] px-3 py-2 text-xs text-[var(--fg-inverse)] disabled:opacity-50"
              >
                {isSavingLearningPolicy ? t('Saving...') : t('Save policy')}
              </button>
            </div>
          </div>
        )}
      </div>

      {/* Хранение данных Section */}
      <div className="bg-[var(--bg-canvas)] rounded-lg border border-[var(--border-subtle)] p-6 shadow-none">
        <h3 className="text-lg font-semibold text-[var(--fg1)] mb-4">{t('Data Storage Locations')}</h3>
        <p className="text-sm text-[var(--fg2)] mb-6">
          {t('View and access where Memento stores your data')}
        </p>

        <div className="space-y-4">
          {/* Database Location */}
          {/* <div className="p-4 border rounded-lg bg-[var(--bg-sheet)]">
            <div className="font-medium mb-2">База данных</div>
            <div className="text-sm text-[var(--fg2)] mb-3 break-all font-mono text-xs">
              {storageLocations?.database || 'Loading...'}
            </div>
            <button
              onClick={() => handleOpenFolder('database')}
              className="flex items-center gap-2 px-3 py-2 text-sm border border-[var(--border-strong)] rounded-md hover:bg-[var(--bg-elevated)] transition-colors"
            >
              <FolderOpen className="w-4 h-4" />
              Open Folder
            </button>
          </div> */}

          {/* Models Location */}
          {/* <div className="p-4 border rounded-lg bg-[var(--bg-sheet)]">
            <div className="font-medium mb-2">Модели Whisper</div>
            <div className="text-sm text-[var(--fg2)] mb-3 break-all font-mono text-xs">
              {storageLocations?.models || 'Loading...'}
            </div>
            <button
              onClick={() => handleOpenFolder('models')}
              className="flex items-center gap-2 px-3 py-2 text-sm border border-[var(--border-strong)] rounded-md hover:bg-[var(--bg-elevated)] transition-colors"
            >
              <FolderOpen className="w-4 h-4" />
              Open Folder
            </button>
          </div> */}

          {/* Recordings Location */}
          <div className="p-4 border rounded-lg bg-[var(--bg-sheet)]">
            <div className="font-medium mb-2">{t('Meeting Recordings')}</div>
            <div className="text-sm text-[var(--fg2)] mb-3 break-all font-mono text-xs">
              {storageLocations?.recordings || t('Loading...')}
            </div>
            <button
              onClick={() => handleOpenFolder('recordings')}
              className="flex items-center gap-2 px-3 py-2 text-sm border border-[var(--border-strong)] rounded-md hover:bg-[var(--bg-elevated)] transition-colors"
            >
              <FolderOpen className="w-4 h-4" />
              {t('Open Folder')}
            </button>
          </div>
        </div>

        <div className="mt-4 p-3 bg-[var(--gold-soft)] rounded-md">
          <p className="text-xs text-[var(--gold)]">
            <strong>{t('Note:')}</strong> {t('Database and models are stored together in your application data directory for unified management.')}
          </p>
        </div>
      </div>

      {/* Analytics Section */}
      <div className="bg-[var(--bg-canvas)] rounded-lg border border-[var(--border-subtle)] p-6 shadow-none">
        <AnalyticsConsentSwitch />
      </div>
    </div>
  )
}
