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
import AnalyticsDataModal from './AnalyticsDataModal';
import { useT } from '@/lib/i18n';

const IconAnalytics = createMaterialSymbol('analytics', 'IconAnalytics');

type FluidIconProps = { size?: number; strokeWidth?: number; className?: string };
const InfoIcon = ({ size = 16, className }: FluidIconProps) => <MaterialSymbol name="info" size={size} weight={400} className={className} />;
const CopyIcon = ({ size = 16, className }: FluidIconProps) => <MaterialSymbol name="content_copy" size={size} weight={400} className={className} />;
const CheckIcon = ({ size = 16, className }: FluidIconProps) => <MaterialSymbol name="check" size={size} weight={400} className={className} />;

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
      await invoke('open_external_url', { url: 'https://github.com/andyzt/meet_at_giga/blob/main/PRIVACY_POLICY.md' });
    } catch (error) {
      console.error('Failed to open privacy policy link:', error);
    }
  };

  // The caption carries the live consent state, the way every other settings
  // cell reports its own status; the static explainer sits in the footer.
  const statusCaption = isProcessing
    ? t('Updating...')
    : isAnalyticsOptedIn && runtimeEnabled
      ? t('Enabled and sending anonymous events to Memento statistics')
      : isAnalyticsOptedIn
        ? t('Enabled, but the analytics client is not connected')
        : t('Disabled — no analytics events are sent');

  return (
    <>
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

      <div className="flex flex-col gap-3 border-t border-[var(--primary-10)] px-4 py-4">
        <p className="settings-cell__caption">
          {t('Usage analytics is enabled automatically to share anonymous product and performance data; no meeting content or personal data is collected. You can turn it off at any time.')}
        </p>

        <Button
          variant="tertiary"
          size="sm"
          leadingIcon={InfoIcon}
          onClick={handleShowDetails}
          className="self-start"
        >
          {t('What analytics collects and where it goes')}
        </Button>

        {/* The install ID is what support asks for; it only exists once analytics is on. */}
        {isAnalyticsOptedIn && userId && (
          <div className="settings-subsection">
            <div className="settings-cell__label">{t('Your User ID')}</div>
            <p className="settings-cell__caption mb-2">
              {t('Share this ID when reporting issues to help us investigate your issue logs')}
            </p>
            <div className="flex items-center gap-2">
              <code className="min-w-0 flex-1 truncate rounded border border-[var(--primary-10)] px-2 py-1 font-mono text-xs text-[var(--fg2)]">
                {userId}
              </code>
              <Button
                variant="tertiary"
                size="sm"
                leadingIcon={isCopied ? CheckIcon : CopyIcon}
                onClick={handleCopyUserId}
                className="shrink-0"
                title={t('Copy User ID')}
              >
                {isCopied ? t('Copied!') : t('Copy')}
              </Button>
            </div>
          </div>
        )}

        <p className="settings-cell__caption">
          {t('Your meetings, transcripts, and recordings remain completely private and local.')}{' '}
          <button
            onClick={handlePrivacyPolicyClick}
            className="underline underline-offset-2 hover:no-underline"
          >
            {t('View Privacy Policy')}
          </button>
        </p>
      </div>

      <AnalyticsDataModal isOpen={showModal} onClose={() => setShowModal(false)} />
    </>
  );
}
