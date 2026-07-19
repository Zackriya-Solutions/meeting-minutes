'use client';
// VALUEOS: the full-bleed electric-blue onboarding screens (no sidebar) — Welcome, Login,
// Blocked, Setup (model download), Storage. Each keeps the exact logic we already ship
// (PKCE login + entitlement gate, reused model download, transcript-folder config); only the
// presentation is the new design. Testids are preserved where the concept maps so tests and
// support diagnostics keep working.
import React, { useEffect, useRef, useState } from 'react';
import { VaMark } from '../../ds/ds';
import { useValueOs } from '../../context/ValueOsProvider';
import type { EntitlementSummary } from '../../auth/authService';
import { getAccessTokenClaims } from '../../debug/tokenClaims';
import { useOnboarding } from '@/contexts/OnboardingContext';
import { IcFolder } from '../icons';

export const VALUEOS_PURCHASE_URL = 'https://www.value-accelerator.io';

function Foot() {
  return <div className="va-foot">Value Accelerator GmbH</div>;
}

/* ── Welcome ─────────────────────────────────────────────────────────────── */
export function WelcomeScreen({ onProceed }: { onProceed: () => void }) {
  return (
    <div className="va-onb va-root" data-testid="valueos-welcome">
      {/* Logo-dominant hero (matches the preferred design): big V✦A mark, then the wordmark. */}
      <VaMark height={160} tone="white" />
      <h1 style={{ fontSize: 48, margin: '10px 0 14px' }}>ValueOS Agent</h1>
      <p>
        Your private, on-device meeting agent. Captures and transcribes meetings locally, then
        feeds them into your ValueOS workflows.
      </p>
      <button className="va-btn va-btn-white" data-testid="valueos-proceed" onClick={onProceed}>
        Get started
      </button>
      <Foot />
    </div>
  );
}

/* ── Login (browser / PKCE) ─────────────────────────────────────────────── */
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

  // Support diagnostic: show the access token's CLAIMS (never the token) so an auth failure
  // can be triaged (client_id / token_use / scopes) and handed to the backend.
  const showDiagnostics = async () => {
    try {
      const c = await getAccessTokenClaims();
      setDiag(c ? JSON.stringify(c, null, 2) : 'No token stored (sign-in did not complete).');
    } catch (e) {
      setDiag(`Diagnostics unavailable: ${(e as Error)?.message ?? String(e)}`);
    }
  };

  return (
    <div className="va-onb va-root" data-testid="valueos-login">
      <VaMark height={44} tone="white" />
      <h1>Sign in to ValueOS</h1>
      <p>
        You&apos;ll sign in securely in your browser. ValueOS Agent requests only read access
        to your tenants, leads and opportunities, and permission to attach transcripts —
        nothing else.
      </p>
      {phase !== 'busy' ? (
        <button className="va-btn va-btn-white" data-testid="valueos-login-start" onClick={signIn}>
          Sign in with ValueOS
        </button>
      ) : (
        <p data-testid="valueos-login-busy" style={{ opacity: 0.92 }}>
          Complete sign-in in your browser…
        </p>
      )}
      {phase === 'error' && (
        <>
          <p className="va-err" data-testid="valueos-login-error">{error}</p>
          <button
            className="va-btn va-btn-outline-white va-btn-sm"
            data-testid="valueos-login-diag"
            style={{ marginTop: 12 }}
            onClick={showDiagnostics}
          >
            Show diagnostics
          </button>
          {diag && (
            <pre
              data-testid="valueos-login-claims"
              className="va-scroll"
              style={{
                textAlign: 'left',
                fontSize: 12,
                lineHeight: 1.5,
                background: 'rgba(0,0,0,0.22)',
                borderRadius: 8,
                padding: '10px 12px',
                marginTop: 12,
                maxWidth: 420,
                maxHeight: 220,
                overflow: 'auto',
                whiteSpace: 'pre-wrap',
                fontFamily: 'var(--font-mono)',
              }}
            >
              {diag}
            </pre>
          )}
        </>
      )}
      <Foot />
    </div>
  );
}

/* ── Entitlement block ──────────────────────────────────────────────────── */
export function BlockedScreen({
  reason,
  onContact,
  onRetry,
}: {
  reason: 'no-membership' | 'no-addon';
  onContact: () => void;
  onRetry: () => void;
}) {
  const msg =
    reason === 'no-membership'
      ? "You don't belong to any ValueOS workspace yet."
      : 'None of your workspaces have the ValueOS Agent add-on. Ask an admin to enable it.';
  return (
    <div className="va-onb va-root" data-testid="valueos-blocked">
      <VaMark height={44} tone="white" />
      <h1>ValueOS Agent access required</h1>
      <p>
        {msg} A workspace with an active ValueOS Agent add-on is required to capture and upload
        meetings. Contact Value Accelerator to get set up.
      </p>
      <button className="va-btn va-btn-white" data-testid="valueos-blocked-contact" onClick={onContact}>
        Contact Value Accelerator
      </button>
      <p className="va-path" data-testid="valueos-blocked-url">{VALUEOS_PURCHASE_URL}</p>
      <button
        className="va-btn va-btn-outline-white va-btn-sm"
        data-testid="valueos-blocked-retry"
        onClick={onRetry}
      >
        Check access again
      </button>
      <Foot />
    </div>
  );
}

/* ── Setup (reused model download) ──────────────────────────────────────── */
export function SetupScreen({ onComplete }: { onComplete: () => void }) {
  const { startBackgroundDownloads, recommendedSummaryModel, parakeetDownloaded, summaryModelDownloaded } =
    useOnboarding();
  const complete = parakeetDownloaded && summaryModelDownloaded;
  const startedRef = useRef(false);

  useEffect(() => {
    if (complete || startedRef.current || !recommendedSummaryModel) return;
    startedRef.current = true;
    void startBackgroundDownloads({
      includeParakeet: true,
      includeSummary: true,
      summaryModel: recommendedSummaryModel,
    });
  }, [complete, recommendedSummaryModel, startBackgroundDownloads]);

  useEffect(() => {
    if (complete) onComplete();
  }, [complete, onComplete]);

  return (
    <div className="va-onb va-root" data-testid="valueos-download">
      <div className="va-spinner" data-testid="valueos-spinner" />
      <h1 style={{ marginTop: 28 }}>Setting up ValueOS Agent</h1>
      <p data-testid="valueos-download-status">
        Preparing on-device transcription. This one-time setup runs the first time you launch —
        it can take a few minutes.
      </p>
      <Foot />
    </div>
  );
}

/* ── Storage (transcript folder) ────────────────────────────────────────── */
export function StorageScreen({ onDone }: { onDone: () => void }) {
  const { config } = useValueOs();
  const [folder, setFolder] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');

  useEffect(() => {
    void config.getTranscriptFolder().then((f) => f && setFolder(f));
  }, [config]);

  const pick = async () => {
    setError('');
    const picked = await config.pickFolder();
    if (picked) setFolder(picked);
  };

  const cont = async () => {
    setError('');
    if (!folder.trim()) {
      setError('Choose a folder where transcripts will be saved.');
      return;
    }
    setBusy(true);
    try {
      const ok = await config.validateWritable(folder);
      if (!ok) {
        setError("That folder isn't writable. Pick another one.");
        return;
      }
      await config.setTranscriptFolder(folder);
      onDone();
    } catch (e) {
      setError((e as Error)?.message ?? 'Could not save the folder.');
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="va-onb va-root" data-testid="valueos-config">
      <VaMark height={44} tone="white" />
      <h1>Where should transcripts live?</h1>
      <p>
        Recordings are transcribed on this device and written here before upload. You can
        change this later in Settings.
      </p>
      <div style={{ width: 'min(460px, 84vw)', display: 'flex', gap: 8 }}>
        <input
          className="va-input va-input-dark"
          data-testid="valueos-config-folder"
          value={folder}
          placeholder="/Users/you/ValueOS Transcripts"
          onChange={(e) => setFolder(e.target.value)}
        />
        <button className="va-btn va-btn-white va-btn-sm" data-testid="valueos-config-pick" onClick={pick}>
          <IcFolder size={15} /> Choose…
        </button>
      </div>
      {error && <p className="va-err" data-testid="valueos-config-error">{error}</p>}
      <button
        className="va-btn va-btn-white"
        data-testid="valueos-config-continue"
        style={{ marginTop: 22 }}
        disabled={busy}
        onClick={cont}
      >
        {busy ? 'Saving…' : 'Continue'}
      </button>
      <Foot />
    </div>
  );
}
