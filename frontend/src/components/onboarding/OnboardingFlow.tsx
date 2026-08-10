import React, { useLayoutEffect, useRef, useState } from 'react';
import { useOnboarding } from '@/contexts/OnboardingContext';
import { WelcomeStep, PermissionsStep, ReadyStep } from './steps';
import { TypeOnboardingWindow, TypeStepTransition } from './type/TypeOnboarding';
import { configureOnboardingWindow } from '@/lib/onboarding-window';
import {
  ONBOARDING_COMPACT_HEIGHT,
  ONBOARDING_EXPANDED_HEIGHT,
  ONBOARDING_FADE_MS,
} from '@/lib/onboarding-transition';

function heightForStep(step: number): number {
  return step === 1 ? ONBOARDING_EXPANDED_HEIGHT : ONBOARDING_COMPACT_HEIGHT;
}

/**
 * Type's compact three-step onboarding composition, backed by Memento's Tauri commands.
 */
export function OnboardingFlow() {
  const { currentStep } = useOnboarding();
  const [displayedStep, setDisplayedStep] = useState(currentStep);
  const [stepVisible, setStepVisible] = useState(true);
  const transitionTokenRef = useRef(0);
  const configuredHeightRef = useRef<number | null>(null);
  const fitsContent = displayedStep >= 2;

  useLayoutEffect(() => {
    const targetHeight = heightForStep(currentStep);

    if (configuredHeightRef.current === null) {
      configuredHeightRef.current = targetHeight;
      void configureOnboardingWindow(targetHeight, { animated: false }).catch((error) => {
        console.error('[onboarding] failed to configure initial window:', error);
      });
      return;
    }

    if (currentStep === displayedStep) return;

    const transitionToken = ++transitionTokenRef.current;

    // The whole transition stays within 300 ms: the old screen fades for the first
    // 150 ms, the new screen fades for the second 150 ms, and the native window resize
    // runs underneath both halves instead of adding another wait between them.
    setStepVisible(false);

    if (Math.abs(configuredHeightRef.current - targetHeight) > 0.5) {
      void configureOnboardingWindow(targetHeight, { animated: true })
        .then(() => {
          if (transitionTokenRef.current === transitionToken) {
            configuredHeightRef.current = targetHeight;
          }
        })
        .catch((error) => {
          console.error('[onboarding] failed to resize window:', error);
        });
    }

    const swapTimer = window.setTimeout(() => {
      if (transitionTokenRef.current !== transitionToken) return;
      setDisplayedStep(currentStep);
      setStepVisible(true);
    }, ONBOARDING_FADE_MS);

    return () => window.clearTimeout(swapTimer);
  }, [currentStep, displayedStep]);

  return (
    <TypeOnboardingWindow fitContent={fitsContent} data-step={displayedStep}>
      <TypeStepTransition visible={stepVisible}>
        {displayedStep === 1 && <WelcomeStep />}
        {displayedStep === 2 && <PermissionsStep />}
        {displayedStep >= 3 && <ReadyStep />}
      </TypeStepTransition>
    </TypeOnboardingWindow>
  );
}
