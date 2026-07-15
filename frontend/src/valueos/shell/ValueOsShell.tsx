'use client';

import React, { useState } from 'react';
import { LandingScreen } from './screens/LandingScreen';
import { ModelDownloadScreen } from './screens/ModelDownloadScreen';
import { StopScreen } from './screens/StopScreen';

// VALUEOS: Branded three-screen shell — the app's entry point for this feature.
//   A. LandingScreen  → B. ModelDownloadScreen → C. StopScreen (flow ends here).
// The download in B REUSES upstream's capability via useOnboarding(); nothing about the
// main meeting UI is wired up (see the hand-off seam in StopScreen).
type Screen = 'landing' | 'download' | 'done';

export function ValueOsShell() {
  const [screen, setScreen] = useState<Screen>('landing');

  return (
    <div data-testid="valueos-shell">
      {screen === 'landing' && (
        <LandingScreen
          onProceed={() => {
            // VALUEOS SEAM (login): a future login/subscription gate belongs between
            // Screen A and Screen B. Today this advances directly (no-op passthrough).
            setScreen('download');
          }}
        />
      )}

      {screen === 'download' && (
        <ModelDownloadScreen onComplete={() => setScreen('done')} />
      )}

      {screen === 'done' && (
        <StopScreen
          onContinue={() => {
            // VALUEOS HAND-OFF: the next feature continues from here. Intentional no-op.
          }}
        />
      )}
    </div>
  );
}
