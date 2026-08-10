'use client';

import React, { useLayoutEffect, useState } from 'react';
import { useOnboarding } from '@/contexts/OnboardingContext';
import { ONBOARDING_PRODUCT_REVEAL_KEY } from '@/lib/onboarding-transition';
import { OnboardingFlow } from './OnboardingFlow';
import styles from './type/TypeOnboarding.module.css';

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
  const [productEntrance, setProductEntrance] = useState<'checking' | 'plain' | 'reveal'>('checking');

  useLayoutEffect(() => {
    let shouldReveal = false;
    try {
      shouldReveal = window.sessionStorage.getItem(ONBOARDING_PRODUCT_REVEAL_KEY) === '1';
      window.sessionStorage.removeItem(ONBOARDING_PRODUCT_REVEAL_KEY);
    } catch {
      // Session storage only coordinates a one-time visual effect.
    }
    setProductEntrance(shouldReveal ? 'reveal' : 'plain');
  }, []);

  if (!statusLoaded || productEntrance === 'checking') {
    return <div className={styles.gateCover} />;
  }

  if (shouldRun) {
    return <OnboardingFlow />;
  }

  return (
    <>
      {children}
      {productEntrance === 'reveal' ? <div className={styles.productRevealCover} aria-hidden="true" /> : null}
    </>
  );
}
