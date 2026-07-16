import React, { useState } from 'react';
import { useValueOs } from '../../context/ValueOsProvider';
import type { EntitlementSummary } from '../../auth/authService';
import { getAccessTokenClaims } from '../../debug/tokenClaims';
import * as ui from './ui';

// VALUEOS: Login (browser/PKCE). The button triggers the auth service, which in Phase 3
// opens the system browser and completes the loopback PKCE exchange; the mock resolves
// immediately. After login we load the entitlement summary and hand it to the shell,
// which decides: entitled → download; not entitled → blocked.
export function LoginScreen({ onDone }: { onDone: (summary: EntitlementSummary) => void }) {
  const { auth } = useValueOs();
  const [phase, setPhase] = useState<'idle' | 'busy' | 'error'>('idle');
  const [error, setError] = useState('');
  const [diag, setDiag] = useState('');

  const signIn = async () => {
    setPhase('busy');
    setError('');
    setDiag('');
    try {
      await auth.login();
      const summary = await auth.loadEntitlementSummary();
      onDone(summary);
    } catch (e) {
      setError((e as Error)?.message ?? 'Sign-in failed');
      setPhase('error');
    }
  };

  // Support diagnostic: show the access token's CLAIMS (never the token/secret) so an auth
  // failure can be triaged (right client_id / token_use / scopes?) and handed to the backend.
  const showDiagnostics = async () => {
    try {
      const c = await getAccessTokenClaims();
      setDiag(c ? JSON.stringify(c, null, 2) : 'No token stored (sign-in did not complete).');
    } catch (e) {
      setDiag(`Diagnostics unavailable: ${(e as Error)?.message ?? String(e)}`);
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
          <>
            <p data-testid="valueos-login-error" style={{ ...ui.sub, color: '#ffd7d7' }}>
              {error}
            </p>
            <button data-testid="valueos-login-diag" style={ui.ghostBtn} onClick={showDiagnostics}>
              Show diagnostics
            </button>
            {diag && (
              <pre
                data-testid="valueos-login-claims"
                style={{
                  textAlign: 'left',
                  fontSize: 12,
                  lineHeight: 1.5,
                  background: 'rgba(0,0,0,0.2)',
                  borderRadius: 8,
                  padding: '10px 12px',
                  marginTop: 10,
                  maxWidth: 380,
                  overflowX: 'auto',
                  whiteSpace: 'pre-wrap',
                }}
              >
                {diag}
              </pre>
            )}
          </>
        )}
      </div>
      <footer style={ui.footer}>Value Accelerator GmbH</footer>
    </div>
  );
}
