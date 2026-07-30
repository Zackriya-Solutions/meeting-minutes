"use client"

import { useEffect, useRef } from "react"
import { Switch } from "./ui/switch"
import Analytics from "@/lib/analytics"
import { useConfig } from "@/contexts/ConfigContext"
import { useT } from '@/lib/i18n'

export function PreferenceSettings() {
  const {
    notificationSettings,
    isLoadingPreferences,
    loadPreferences,
    updateNotificationSettings
  } = useConfig();

  const t = useT();

  const hasTrackedViewRef = useRef(false);

  // Lazy load preferences on mount (only loads if not already cached)
  useEffect(() => {
    loadPreferences();
    // Reset tracking ref on mount (every tab visit)
    hasTrackedViewRef.current = false;
  }, [loadPreferences]);

  // Track preferences viewed analytics on every tab visit (once per mount).
  useEffect(() => {
    if (hasTrackedViewRef.current) return;
    hasTrackedViewRef.current = true;
    void Analytics.track('preferences_viewed');
  }, []);

  // Show loading only if we're actually loading and don't have cached data
  if (isLoadingPreferences && !notificationSettings) {
    return <div className="settings-section">{t('Loading Preferences...')}</div>
  }

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

  return (
    <div>
      {/* Detection evidence remains in memory; only normalized auto-listening lifecycle is stored. */}
      <section className="settings-section">
        <div className="flex items-start justify-between gap-4 sm:gap-6">
          <div className="min-w-0">
            <h3 className="text-lg font-semibold text-foreground mb-2">{t('Automatic meeting detection')}</h3>
            <p className="text-sm text-muted-foreground">
              {t('Detect supported meeting apps and browser calls locally. Supported native clients start a local recording automatically; browser calls still require confirmation. Raw process names, window titles, and URLs are not stored.')}
            </p>
          </div>
          <Switch
            className="shrink-0"
            checked={notificationSettings?.auto_meeting_detection ?? true}
            disabled={!notificationSettings}
            onCheckedChange={handleAutoMeetingDetectionChange}
          />
        </div>
      </section>

    </div>
  )
}
