import { useCallback, useState } from 'react';
import { toast } from 'sonner';
import { useOnboarding } from '@/contexts/OnboardingContext';
import { useT } from '@/lib/i18n';

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
    try {
      const demoMeetingId = await completeOnboarding();
      window.location.href = demoMeetingId
        ? `/meeting-details?id=${encodeURIComponent(demoMeetingId)}`
        : '/';
    } catch (error) {
      console.error('[onboarding] failed to complete setup:', error);
      toast.error(t('Failed to complete setup'), { description: t('Please try again.') });
      setIsFinishing(false);
    }
  }, [completeOnboarding, t]);

  return { finish, isFinishing };
}
