'use client';

import React from 'react';
import { ValueOsProvider, type ValueOsServices } from '../context/ValueOsProvider';
import { DesignSystem } from '../ds/ds';
import { AppFlow } from '../app/AppFlow';
import { BuildStamp } from './BuildStamp';

// VALUEOS: the app root. Injects the Value Accelerator design system, the build stamp (on
// every screen), and hosts the redesigned flow (AppFlow) — onboarding → dark-sidebar app.
// Services are injected via ValueOsProvider (mock in tests, real transport in the packaged
// app). The heavy lifting lives in ../app/*; this file is just composition.
export function ValueOsShell({ services }: { services?: ValueOsServices }) {
  return (
    <ValueOsProvider services={services}>
      <div data-testid="valueos-shell">
        <DesignSystem />
        {/* Hide upstream download/status toasts (top-right) that leak model names. */}
        <style>{'[data-sonner-toaster]{display:none !important;}'}</style>
        <BuildStamp />
        <AppFlow />
      </div>
    </ValueOsProvider>
  );
}
