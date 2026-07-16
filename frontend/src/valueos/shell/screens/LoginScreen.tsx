import React, { useState } from 'react';
import { useValueOs } from '../../context/ValueOsProvider';
import type { EntitlementSummary } from '../../auth/authService';
import * as ui from './ui';

// VALUEOS: Login (browser/PKCE). The button triggers the auth service, which in Phase 3
// opens the system browser and completes the loopback PKCE exchange; the mock resolves
// immediately. After login we load the entitlement summary and hand it to the shell,
// which decides: entitled → download; not entitled → blocked.
export function LoginScreen({ onDone }: { onDone: (summary: EntitlementSummary) => void }) {
  const { auth } = useValueOs();
  const [phase, setPhase] = useState<'idle' | 'busy' | 'error'>('idle');
  const [error, setError] = useState('');

  const signIn = async () => {
    setPhase('busy');
    setError('');
    try {
      await auth.login();
      const summary = await auth.loadEntitlementSummary();
      onDone(summary);
    } catch (e) {
      setError((e as Error)?.message ?? 'Sign-in failed');
      setPhase('error');
    }
  };

  return (
    <div data-testid="valueos-login" style={ui.page}>
      <div style={ui.card}>
        <h1 style={ui.h1}>Sign in to ValueOS</h1>
        <p style={ui.sub}>
          You&apos;ll sign in securely in your browser. ValueOS Agent requests only read
          access to your tenants, leads and opportunities, and permission to attach
          transcripts — nothing else.
        </p>
        {phase !== 'busy' ? (
          <button data-testid="valueos-login-start" style={ui.primaryBtn} onClick={signIn}>
            Sign in with ValueOS
          </button>
        ) : (
          <p data-testid="valueos-login-busy" style={ui.sub}>
            Complete sign-in in your browser…
          </p>
        )}
        {phase === 'error' && (
          <p data-testid="valueos-login-error" style={{ ...ui.sub, color: '#ffd7d7' }}>
            {error}
          </p>
        )}
      </div>
      <footer style={ui.footer}>Value Accelerator GmbH</footer>
    </div>
  );
}
