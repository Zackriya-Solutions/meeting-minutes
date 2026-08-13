import { useCallback, useState } from 'react';
import { toast } from 'sonner';
import { useOnboarding } from '@/contexts/OnboardingContext';
import { useT } from '@/lib/i18n';
import { configureOnboardingWindow, restoreMainWindow } from '@/lib/onboarding-window';
import {
  ONBOARDING_COMPACT_HEIGHT,
  ONBOARDING_FADE_MS,
  ONBOARDING_PRODUCT_REVEAL_KEY,
} from '@/lib/onboarding-transition';

/**
 * Finish setup and land the user somewhere that explains the product.
 *
 * Onboarding seeds an example meeting, and opening it is the point: an empty meeting list
 * says nothing about what a recording turns into. If seeding did not happen (an install that
 * already had the example, or deleted it), fall back to the home screen.
 *
 * Navigation is a full page load, not a router push — the config, sidebar, and recording
 * providers all read their state once on mount, and they mounted before setup wrote it.
 */
export function useFinishOnboarding() {
  const { completeOnboarding } = useOnboarding();
  const t = useT();
  const [isFinishing, setIsFinishing] = useState(false);

  const finish = useCallback(async () => {
    setIsFinishing(true);
    let resizePromise: Promise<void> | null = null;

    try {
      // Start persistence immediately, then give WebKit one paint boundary to commit the
      // exit fade. Native resize and setup work continue together instead of forming two
      // separate waits.
      const completionPromise = completeOnboarding();
      await new Promise<void>((resolve) => {
        requestAnimationFrame(() => resolve());
      });

      resizePromise = restoreMainWindow({ animated: true });
      const [demoMeetingId] = await Promise.all([
        completionPromise,
        resizePromise,
        new Promise<void>((resolve) => window.setTimeout(resolve, ONBOARDING_FADE_MS)),
      ]);

      try {
        window.sessionStorage.setItem(ONBOARDING_PRODUCT_REVEAL_KEY, '1');
      } catch {
        // A disabled storage backend should not block setup completion.
      }

      window.location.href = demoMeetingId
        ? `/meeting-details?id=${encodeURIComponent(demoMeetingId)}`
        : '/';
    } catch (error) {
      console.error('[onboarding] failed to complete setup:', error);
      try {
        window.sessionStorage.removeItem(ONBOARDING_PRODUCT_REVEAL_KEY);
      } catch {
        // No-op: storage is an enhancement for the one-time reveal only.
      }
      if (resizePromise) await resizePromise.catch(() => undefined);
      await configureOnboardingWindow(ONBOARDING_COMPACT_HEIGHT, { animated: true }).catch(
        (resizeError) => console.warn('[onboarding] failed to restore compact window:', resizeError),
      );
      toast.error(t('Failed to complete setup'), { description: t('Please try again.') });
      setIsFinishing(false);
    }
  }, [completeOnboarding, t]);

  return { finish, isFinishing };
}
