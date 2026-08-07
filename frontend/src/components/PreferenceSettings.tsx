"use client"

import { useEffect, useState } from "react"
import { Switch } from "./ui/switch"
import { Button } from "./ui/button"
import { FolderOpen } from "lucide-react"
import { invoke } from "@tauri-apps/api/core"
import { useConfig, NotificationSettings } from "@/contexts/ConfigContext"

export function PreferenceSettings() {
  const {
    notificationSettings,
    storageLocations,
    isLoadingPreferences,
    loadPreferences,
    updateNotificationSettings
  } = useConfig();

  const [notificationsEnabled, setNotificationsEnabled] = useState<boolean | null>(null);
  const [isInitialLoad, setIsInitialLoad] = useState(true);
  const [previousNotificationsEnabled, setPreviousNotificationsEnabled] = useState<boolean | null>(null);

  // Lazy load preferences on mount (only loads if not already cached)
  useEffect(() => {
    loadPreferences();
  }, [loadPreferences]);

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

    } catch (error) {
      console.error(`Failed to open ${folderType} folder:`, error);
    }
  };

  if (
    (isLoadingPreferences && !notificationSettings && !storageLocations) ||
    (notificationsEnabled === null && !isLoadingPreferences)
  ) {
    return (
      <div className="space-y-6" role="status" aria-label="Loading preferences">
        {[0, 1, 2].map((i) => (
          <div key={i} className="space-y-2 border-b border-line pb-6">
            <div className="skeleton h-4 w-32" />
            <div className="skeleton h-3 w-72" />
          </div>
        ))}
      </div>
    )
  }

  const notificationsEnabledValue = notificationsEnabled ?? false;

  // Sections separated by hairlines rather than stacked cards. Settings is a
  // list of decisions, not a gallery — and a card inside a card is never right.
  return (
    <div className="divide-y divide-line">
      <section className="flex flex-wrap items-center justify-between gap-3 pb-6">
        <div>
          <h2 className="text-base font-medium text-ink">Notifications</h2>
          <p className="mt-0.5 max-w-[54ch] text-sm text-ink-muted">
            Show a system notification when a recording starts and stops.
          </p>
        </div>
        <Switch
          checked={notificationsEnabledValue}
          onCheckedChange={setNotificationsEnabled}
          aria-label="Recording notifications"
        />
      </section>

      <section className="py-6">
        <h2 className="text-base font-medium text-ink">Where your data lives</h2>
        <p className="mt-0.5 max-w-[62ch] text-sm text-ink-muted">
          Everything stays on this machine. Recordings, the transcript database,
          and downloaded models are all in your application data directory.
        </p>

        <div className="mt-4 flex flex-wrap items-center gap-3">
          <div className="min-w-0 flex-1">
            <p className="text-sm font-medium text-ink">Meeting recordings</p>
            <p className="readout mt-0.5 break-all text-2xs text-ink-muted">
              {storageLocations?.recordings || '…'}
            </p>
          </div>
          <Button variant="outline" size="sm" onClick={() => handleOpenFolder('recordings')}>
            <FolderOpen aria-hidden />
            Open folder
          </Button>
        </div>
      </section>
    </div>
  )
}
