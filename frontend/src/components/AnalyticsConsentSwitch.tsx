import { useContext, useEffect, useState } from 'react';
import { Switch } from '@/components/ui/switch';
import { Button } from '@/components/ui/fluid-button';
import {
  MaterialSymbol,
  createMaterialSymbol,
} from '@/vendor/deslop/primitives/material-symbols-react';
import { ANALYTICS_PREFERENCE_MIGRATION_KEY, AnalyticsContext } from './AnalyticsProvider';
import { load } from '@tauri-apps/plugin-store';
import { invoke } from '@tauri-apps/api/core';
import { Analytics } from '@/lib/analytics';
import { useT } from '@/lib/i18n';

const IconAnalytics = createMaterialSymbol('analytics', 'IconAnalytics');
const IconUserId = createMaterialSymbol('badge', 'IconUserId');

type FluidIconProps = { size?: number; strokeWidth?: number; className?: string };
const CopyIcon = ({ size = 16, className }: FluidIconProps) => <MaterialSymbol name="content_copy" size={size} weight={400} className={className} />;
const CheckIcon = ({ size = 16, className }: FluidIconProps) => <MaterialSymbol name="check" size={size} weight={400} className={className} />;

export default function AnalyticsConsentSwitch() {
  const t = useT();
  const { setIsAnalyticsOptedIn, isAnalyticsOptedIn } = useContext(AnalyticsContext);
  const [isProcessing, setIsProcessing] = useState(false);
  const [userId, setUserId] = useState<string>('');
  const [isCopied, setIsCopied] = useState(false);
  const [runtimeEnabled, setRuntimeEnabled] = useState(false);

  // Note: Store loading is handled by AnalyticsProvider to avoid race conditions

  useEffect(() => {
    const loadUserId = async () => {
      try {
        const id = await Analytics.getPersistentUserId();
        setUserId(id);
      } catch (error) {
        console.error('Failed to load user ID:', error);
      }
    };
    loadUserId();
  }, []);

  useEffect(() => {
    Analytics.isEnabled().then(setRuntimeEnabled).catch(() => setRuntimeEnabled(false));
  }, [isAnalyticsOptedIn, isProcessing]);

  const handleCopyUserId = async () => {
    if (!userId) return;

    try {
      await navigator.clipboard.writeText(userId);
      setIsCopied(true);
      setTimeout(() => setIsCopied(false), 1500);

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
          analyticsOptedIn: true
        }
      });
      await store.set('analyticsOptedIn', enabled);
      await store.set(ANALYTICS_PREFERENCE_MIGRATION_KEY, true);
      await store.save();

      if (enabled) {
        // Full analytics initialization (same as AnalyticsProvider)
        const userId = await Analytics.getPersistentUserId();

        // Initialize analytics
        await Analytics.init();

        // Identify user with enhanced properties immediately after init
        await Analytics.identify(userId, {
          app_version: '0.5.0',
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

  const statusCaption = isProcessing
    ? t('Updating...')
    : isAnalyticsOptedIn && runtimeEnabled
      ? t('Enabled and sending anonymous events to Memento statistics')
      : isAnalyticsOptedIn
        ? t('Enabled, but the analytics client is not connected')
        : t('Disabled — no analytics events are sent');

  return (
    <>
      <section className="settings-section settings-cell">
        <div className="settings-cell__row">
          <span className="settings-cell__avatar" aria-hidden="true">
            <IconAnalytics size={20} weight={400} />
          </span>
          <div className="settings-cell__text">
            <h3 className="settings-cell__label">{t('Usage Analytics')}</h3>
            <p className="settings-cell__caption">{statusCaption}</p>
          </div>
          <Switch
            className="shrink-0"
            checked={isAnalyticsOptedIn}
            onCheckedChange={handleToggle}
            disabled={isProcessing}
            aria-label={t('Usage Analytics')}
          />
        </div>
      </section>

      <section className="settings-section settings-cell">
        <div className="settings-cell__row">
          <span className="settings-cell__avatar" aria-hidden="true">
            <IconUserId size={20} weight={400} />
          </span>
          <div className="settings-cell__text">
            <h3 className="settings-cell__label">{t('Your User ID')}</h3>
            <p className="settings-cell__caption truncate" title={userId}>
              {userId || t('Loading...')}
            </p>
          </div>
          <Button
            variant="ghost"
            size="icon"
            onClick={handleCopyUserId}
            className="fluid-select-token-surface fluid-select-token-trigger !h-9 !w-9 shrink-0 !rounded-lg border border-[var(--primary-10)] bg-[var(--elevation-2)] text-[var(--deslop-primary)] data-[copy-success=true]:!opacity-100 [&>span:first-child]:!bg-transparent"
            disabled={!userId || isCopied}
            data-copy-success={isCopied}
            aria-label={isCopied ? t('Copied!') : t('Copy User ID')}
            title={isCopied ? t('Copied!') : t('Copy User ID')}
          >
            <span className="relative block h-4 w-4" aria-hidden="true">
              <span
                className={`absolute inset-0 flex items-center justify-center transition-[opacity,filter,transform] duration-200 ease-out ${
                  isCopied
                    ? 'scale-100 opacity-100 blur-0'
                    : 'scale-[0.7] opacity-0 blur-[2px]'
                }`}
              >
                <CheckIcon size={16} />
              </span>
              <span
                className={`absolute inset-0 flex items-center justify-center transition-[opacity,filter,transform] duration-200 ease-out ${
                  isCopied
                    ? 'scale-0 opacity-0 blur-[2px]'
                    : 'scale-100 opacity-100 blur-0'
                }`}
              >
                <CopyIcon size={16} />
              </span>
            </span>
          </Button>
        </div>
      </section>
    </>
  );
}
