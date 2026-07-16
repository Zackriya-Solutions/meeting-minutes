'use client';

import React, { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { ValueOsProvider, type ValueOsServices } from '../context/ValueOsProvider';
import { LandingScreen } from './screens/LandingScreen';
import { LoginScreen } from './screens/LoginScreen';
import { EntitlementBlockedScreen, VALUEOS_PURCHASE_URL } from './screens/EntitlementBlockedScreen';
import { ModelDownloadScreen } from './screens/ModelDownloadScreen';
import { ConfigScreen } from './screens/ConfigScreen';
import { HomeScreen } from './screens/HomeScreen';
import { CaptureScreen } from './screens/CaptureScreen';
import { FinalizeScreen } from './screens/FinalizeScreen';
import type { EntitledTenant, EntitlementSummary } from '../auth/authService';
import type { CaptureResult } from './flowTypes';

// VALUEOS: the full flow —
//   get-started → login (+entitlement gate) → model download → config (folder)
//   → capture (blocking tenant+type+target) → finalize (store + digest + upload)
//   → back to capture for the next meeting.
// All in our namespace; services injected via ValueOsProvider (mock today, real in Phase 3).
type Screen = 'landing' | 'login' | 'blocked' | 'download' | 'config' | 'home' | 'capture' | 'finalize';

export function ValueOsShell({ services }: { services?: ValueOsServices }) {
  return (
    <ValueOsProvider services={services}>
      <Flow />
    </ValueOsProvider>
  );
}

function Flow() {
  const [screen, setScreen] = useState<Screen>('landing');
  const [entitled, setEntitled] = useState<EntitledTenant[]>([]);
  const [blockedReason, setBlockedReason] = useState<'no-membership' | 'no-addon'>('no-addon');
  const [capture, setCapture] = useState<CaptureResult | null>(null);

  const contactSales = () => {
    // Opens the system browser via the existing (already-registered) command.
    void invoke('open_external_url', { url: VALUEOS_PURCHASE_URL }).catch(() => {});
  };

  const handleLogin = (summary: EntitlementSummary) => {
    if (summary.anyEntitled) {
      setEntitled(summary.entitled);
      setScreen('download');
      return;
    }
    // Gate (contract §2): no workspace has the agent add-on active → hard block, no bypass.
    // total_memberships tells us whether they're a member of nothing vs. a member of
    // workspaces that lack the add-on, so we can word it correctly.
    setBlockedReason(summary.totalMemberships === 0 ? 'no-membership' : 'no-addon');
    setScreen('blocked');
  };

  return (
    <div data-testid="valueos-shell">
      {/* VALUEOS: hide upstream download/status toasts (top-right) that leak model names */}
      <style>{'[data-sonner-toaster]{display:none !important;}'}</style>
      {screen === 'landing' && <LandingScreen onProceed={() => setScreen('login')} />}

      {screen === 'login' && <LoginScreen onDone={handleLogin} />}

      {screen === 'blocked' && (
        <EntitlementBlockedScreen
          reason={blockedReason}
          onContact={contactSales}
          onRetry={() => setScreen('login')}
        />
      )}

      {screen === 'download' && <ModelDownloadScreen onComplete={() => setScreen('config')} />}

      {screen === 'config' && <ConfigScreen onDone={() => setScreen('home')} />}

      {screen === 'home' && <HomeScreen onNew={() => setScreen('capture')} />}

      {screen === 'capture' && (
        <CaptureScreen
          entitledTenants={entitled}
          onFinish={(r) => {
            setCapture(r);
            setScreen('finalize');
          }}
        />
      )}

      {screen === 'finalize' && capture && (
        <FinalizeScreen
          capture={capture}
          onDone={() => {
            setCapture(null);
            setScreen('home'); // back to the transcripts list
          }}
          onReauth={() => setScreen('login')}
        />
      )}
    </div>
  );
}
