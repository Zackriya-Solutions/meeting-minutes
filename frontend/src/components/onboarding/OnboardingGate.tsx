'use client';

import React from 'react';
import { useOnboarding } from '@/contexts/OnboardingContext';
import { OnboardingFlow } from './OnboardingFlow';

/**
 * Shows setup instead of the app on a fresh install.
 *
 * The app tree stays unmounted while setup runs — auto-listening, the recording pill and the
 * sidebar all assume a configured install, and one of them starting a background recording
 * mid-setup is worse than a moment of nothing. While the saved status is still being read we
 * render nothing rather than the app: a first launch that flashes an empty meeting list and
 * then replaces it with setup looks like a bug.
 */
export function OnboardingGate({ children }: { children: React.ReactNode }) {
  const { statusLoaded, shouldRun } = useOnboarding();

  if (!statusLoaded) {
    return <div className="fixed inset-0 bg-background" />;
  }

  if (shouldRun) {
    return <OnboardingFlow />;
  }

  return <>{children}</>;
}
