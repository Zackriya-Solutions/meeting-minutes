import React from 'react';
import { useOnboarding } from '@/contexts/OnboardingContext';
import { WelcomeStep, ModelSetupStep, PermissionsStep } from './steps';

/**
 * Three steps on macOS, two elsewhere:
 *   1. Welcome — what a meeting turns into
 *   2. Setup — one model package (auto-starting) + the DeepSeek tier for summaries
 *   3. Permissions — microphone and system audio (macOS only)
 *
 * Setup finishes the flow on platforms without the permissions step.
 */
export function OnboardingFlow() {
  const { currentStep, isMac } = useOnboarding();

  return (
    <div className="onboarding-flow">
      {currentStep === 1 && <WelcomeStep />}
      {currentStep === 2 && <ModelSetupStep />}
      {currentStep >= 3 && isMac && <PermissionsStep />}
    </div>
  );
}
