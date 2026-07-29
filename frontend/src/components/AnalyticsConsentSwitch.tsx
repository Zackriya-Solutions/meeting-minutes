import React, { useContext, useState, useEffect } from 'react';
import { Switch } from '@/components/ui/switch';
import { Button } from '@/components/ui/button';
import { Info, Loader2, Copy, Check } from '@/components/deslop-icons';
import { AnalyticsContext } from './AnalyticsProvider';
import { load } from '@tauri-apps/plugin-store';
import { invoke } from '@tauri-apps/api/core';
import { Analytics } from '@/lib/analytics';
import AnalyticsDataModal from './AnalyticsDataModal';
import { useT } from '@/lib/i18n';

const ANALYTICS_DEFAULT_OFF_MIGRATION_KEY = 'analyticsDefaultOffMigrationV1';

export default function AnalyticsConsentSwitch() {
  const t = useT();
  const { setIsAnalyticsOptedIn, isAnalyticsOptedIn } = useContext(AnalyticsContext);
  const [isProcessing, setIsProcessing] = useState(false);
  const [showModal, setShowModal] = useState(false);
  const [userId, setUserId] = useState<string>('');
  const [isCopied, setIsCopied] = useState(false);
  const [runtimeEnabled, setRuntimeEnabled] = useState(false);

  // Note: Store loading is handled by AnalyticsProvider to avoid race conditions

  useEffect(() => {
    const loadUserId = async () => {
      if (isAnalyticsOptedIn) {
        try {
          const id = await Analytics.getPersistentUserId();
          setUserId(id);
        } catch (error) {
          console.error('Failed to load user ID:', error);
        }
      } else {
        setUserId('');
      }
    };
    loadUserId();
  }, [isAnalyticsOptedIn]);

  useEffect(() => {
    Analytics.isEnabled().then(setRuntimeEnabled).catch(() => setRuntimeEnabled(false));
  }, [isAnalyticsOptedIn, isProcessing]);

  const handleCopyUserId = async () => {
    if (!userId) return;

    try {
      await navigator.clipboard.writeText(userId);
      setIsCopied(true);
      setTimeout(() => setIsCopied(false), 2000);

      // Track that user copied their ID
      await Analytics.track('user_id_copied', {
        user_id: userId
      });
    } catch (error) {
      console.error('Failed to copy user ID:', error);
    }
  };

  const handleToggle = async (enabled: boolean) => {
    await performToggle(enabled);
  };

  const performToggle = async (enabled: boolean) => {
    // Optimistic update - immediately update UI state
    setIsAnalyticsOptedIn(enabled);
    setIsProcessing(true);

    try {
      const store = await load('analytics.json', {
        autoSave: false,
        defaults: {
          analyticsOptedIn: false
        }
      });
      await store.set('analyticsOptedIn', enabled);
      await store.set(ANALYTICS_DEFAULT_OFF_MIGRATION_KEY, true);
      await store.save();

      if (enabled) {
        // Full analytics initialization (same as AnalyticsProvider)
        const userId = await Analytics.getPersistentUserId();

        // Initialize analytics
        await Analytics.init();

        // Identify user with enhanced properties immediately after init
        await Analytics.identify(userId, {
          app_version: '0.4.0',
          platform: 'tauri',
          first_seen: new Date().toISOString(),
          os: navigator.platform,
        });

        // Start analytics session with the same user ID
        await Analytics.startSession(userId);

        // Track app started (re-enabled)
        await Analytics.trackAppStarted();

        // Track that user enabled analytics
        try {
          await invoke('track_analytics_enabled');
        } catch (error) {
          console.error('Failed to track analytics enabled:', error);
        }

        console.log('Analytics re-enabled successfully');
      } else {
        // Track that user disabled analytics BEFORE disabling
        try {
          await invoke('track_analytics_disabled');
        } catch (error) {
          console.error('Failed to track analytics disabled:', error);
        }

        await Analytics.disable();
        setRuntimeEnabled(false);
        console.log('Analytics disabled successfully');
      }
    } catch (error) {
      console.error('Failed to toggle analytics:', error);
      // Revert the optimistic update on error
      setIsAnalyticsOptedIn(!enabled);
      // You could also show a toast notification here to inform the user
    } finally {
      setIsProcessing(false);
    }
  };

  const handleShowDetails = async () => {
    setShowModal(true);
    if (!isAnalyticsOptedIn) return;
    try {
      await invoke('track_analytics_transparency_viewed');
    } catch (error) {
      console.error('Failed to track transparency view:', error);
    }
  };

  const handlePrivacyPolicyClick = async () => {
    try {
      await invoke('open_external_url', { url: 'https://github.com/Zackriya-Solutions/meeting-minutes/blob/main/PRIVACY_POLICY.md' });
    } catch (error) {
      console.error('Failed to open privacy policy link:', error);
    }
  };

  return (
    <>
      <div className="space-y-4">
        <div>
          <h3 className="text-base font-semibold text-foreground mb-2">{t('Usage Analytics')}</h3>
          <p className="text-sm text-muted-foreground mb-4">
            {t('Usage analytics is off by default. You can turn it on to share anonymous product and performance data; no personal content is collected.')}
          </p>
        </div>

        <div className="flex items-center justify-between p-3 bg-background rounded-lg border border-border">
          <div>
            <h4 className="font-semibold text-foreground">{t('Enable Analytics')}</h4>
            <p className="text-sm text-muted-foreground">
              {isProcessing
                ? t('Updating...')
                : isAnalyticsOptedIn && runtimeEnabled
                  ? t('Enabled and sending anonymous events to PostHog')
                  : isAnalyticsOptedIn
                    ? t('Enabled, but the analytics client is not connected')
                    : t('Disabled — no analytics events are sent')}
            </p>
          </div>
          <div className="flex items-center gap-2 ml-4">
            {isProcessing && (
              <Loader2 className="w-4 h-4 animate-spin text-muted-foreground" />
            )}
            <Switch
              checked={isAnalyticsOptedIn}
              onCheckedChange={handleToggle}
              disabled={isProcessing}
            />
          </div>
        </div>

        <Button variant="outline" size="sm" onClick={handleShowDetails}>
          <Info className="h-4 w-4" />
          {t('What analytics collects and where it goes')}
        </Button>

        {/* User ID Display */}
        {isAnalyticsOptedIn && userId && (
          <div className="p-4 border rounded-lg bg-background">
            <div className="flex items-start justify-between gap-4">
              <div className="flex-1 min-w-0">
                <div className="font-medium text-foreground mb-1">{t('Your User ID')}</div>
                <p className="text-xs text-muted-foreground mb-2">
                  {t('Share this ID when reporting issues to help us investigate your issue logs')}
                </p>
                <div className="flex items-center gap-2">
                  <code className="text-xs text-muted-foreground bg-background px-2 py-1 rounded border border-border font-mono flex-1 truncate">
                    {userId}
                  </code>
                  <Button
                    onClick={handleCopyUserId}
                    variant="outline"
                    size="sm"
                    className="flex-shrink-0"
                    title={t('Copy User ID')}
                  >
                    {isCopied ? (
                      <>
                        <Check className="w-3.5 h-3.5 text-success" />
                        <span className="text-success">{t('Copied!')}</span>
                      </>
                    ) : (
                      <>
                        <Copy className="w-3.5 h-3.5" />
                        <span>{t('Copy')}</span>
                      </>
                    )}
                  </Button>
                </div>
              </div>
            </div>
          </div>
        )}

        <div className="flex items-start gap-2 p-2 bg-primary/10 rounded border border-primary/40">
          <Info className="w-4 h-4 text-primary mt-0.5 flex-shrink-0" />
          <div className="text-xs text-primary">
            <p className="mb-1">
              {t('Your meetings, transcripts, and recordings remain completely private and local.')}
            </p>
            <button
              onClick={handlePrivacyPolicyClick}
              className="text-primary hover:text-primary/90 underline hover:no-underline"
            >
              {t('View Privacy Policy')}
            </button>
          </div>
        </div>
      </div>

      {/* 2-Step Opt-Out Modal */}
      <AnalyticsDataModal
        isOpen={showModal}
        onClose={() => setShowModal(false)}
      />
    </>
  );
}
