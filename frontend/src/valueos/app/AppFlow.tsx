'use client';
// VALUEOS: the redesigned end-to-end flow.
//   welcome → browser/PKCE login (+entitlement gate) → setup (reused model download) →
//   storage (transcript folder, first run only) → main app.
// The main app is the dark-sidebar shell hosting Dashboard / Transcripts / Settings and the
// full-width Recording screen, plus the New-transcript wizard. All existing logic is reused
// verbatim (auth, gate, config, composite /calls upload via finalizeCall, local history).
//
// CONSTRAINT (one ongoing transcript): there can be only ONE on-air call. While a call is
// recording, "New transcript" is blocked with an explicit error telling the user to end the
// current one first.
import React, { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useValueOs } from '../context/ValueOsProvider';
import type { EntitledTenant, EntitlementSummary } from '../auth/authService';
import type { TranscriptRecord } from '../history/transcriptHistory';
import type { CaptureResult } from '../shell/flowTypes';
import { finalizeCall } from '../upload/finalizeCall';
import { BugReportDialog } from '../bugreport/BugReportDialog';
import { AppShell, type MainRoute } from './AppShell';
import { Dashboard } from './screens/Dashboard';
import { Transcripts } from './screens/Transcripts';
import { Settings } from './screens/Settings';
import { Recording } from './screens/Recording';
import { Wizard } from './Wizard';
import {
  WelcomeScreen,
  LoginScreen,
  BlockedScreen,
  SetupScreen,
  StorageScreen,
  VALUEOS_PURCHASE_URL,
} from './onboarding/Onboarding';
import type { ActiveCall, StartCallMeta } from './types';

type Stage = 'welcome' | 'login' | 'blocked' | 'setup' | 'storage' | 'main';

const ONGOING_ERROR =
  'A transcript is already in progress. End it (End & upload) before starting a new one — only one call can be recorded at a time.';

export function AppFlow() {
  const { auth, config, digest, uploadQueue, history, updater } = useValueOs();
  const [stage, setStage] = useState<Stage>('welcome');
  const [route, setRoute] = useState<MainRoute>('dashboard');
  const [entitled, setEntitled] = useState<EntitledTenant[]>([]);
  const [blockedReason, setBlockedReason] = useState<'no-membership' | 'no-addon'>('no-addon');

  const [records, setRecords] = useState<TranscriptRecord[]>([]);
  const [activeCall, setActiveCall] = useState<ActiveCall | null>(null);
  const [callStarted, setCallStarted] = useState(false);
  const [wizardOpen, setWizardOpen] = useState(false);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [guardError, setGuardError] = useState<string | null>(null);
  const [fileWarning, setFileWarning] = useState<string | null>(null);
  // Bug report is reachable from the sidebar "Report a bug" utility item (fast path, every
  // screen) AND the Settings → Help card (fallback). The dialog is hoisted here so ONE instance
  // serves both entry points.
  const [bugOpen, setBugOpen] = useState(false);

  const refreshRecords = useCallback(async () => {
    setRecords(await history.list());
  }, [history]);

  useEffect(() => {
    if (stage === 'main') void refreshRecords();
  }, [stage, refreshRecords]);

  // WS4: on entering the app, register the install once (+ report update_success if we came
  // up on a newer version than last run), then heartbeat on a long interval. Best-effort —
  // telemetry never blocks the user, and this touches no user data.
  const registeredRef = useRef(false);
  useEffect(() => {
    if (stage !== 'main') return;
    const tenantId = entitled[0]?.tenant.id;
    if (!tenantId) return;
    if (!registeredRef.current) {
      registeredRef.current = true;
      void updater.registerAndReconcile(tenantId);
    }
    const id = setInterval(() => void updater.heartbeat(tenantId), 6 * 60 * 60 * 1000);
    return () => clearInterval(id);
  }, [stage, entitled, updater]);

  const contactSales = () => {
    void invoke('open_external_url', { url: VALUEOS_PURCHASE_URL }).catch(() => {});
  };

  // The post-login entitlement gate (GET /me/agent-tenants).
  const applySummary = (summary: EntitlementSummary, next: () => void) => {
    if (summary.anyEntitled) {
      setEntitled(summary.entitled);
      next();
    } else {
      setBlockedReason(summary.totalMemberships === 0 ? 'no-membership' : 'no-addon');
      setStage('blocked');
    }
  };

  const handleLogin = (summary: EntitlementSummary) => applySummary(summary, () => setStage('setup'));

  // Re-run the gate when a workspace loses the add-on mid-session (§2.7).
  const reGate = () => {
    void (async () => {
      try {
        const summary = await auth.loadEntitlementSummary();
        applySummary(summary, () => setStage('main'));
      } catch {
        setStage('login');
      }
    })();
  };

  // After model setup: transcript-folder config is a ONE-TIME step (first run only).
  const afterSetup = () => {
    void (async () => {
      const folder = await config.getTranscriptFolder();
      setStage(folder ? 'main' : 'storage');
    })();
  };

  const logout = () => {
    void (async () => {
      try {
        await auth.logout();
      } catch {
        /* ignore */
      }
      setActiveCall(null);
      setCallStarted(false);
      setWizardOpen(false);
      setSelectedId(null);
      setEntitled([]);
      setRecords([]);
      setStage('welcome');
      setRoute('dashboard');
    })();
  };

  // ── one-ongoing-transcript guard ──────────────────────────────────────────
  const requestNew = () => {
    if (activeCall) {
      setGuardError(ONGOING_ERROR);
      return;
    }
    setGuardError(null);
    setWizardOpen(true);
  };

  const startCall = (meta: StartCallMeta) => {
    setWizardOpen(false);
    setCallStarted(false);
    setActiveCall({ meta, startedAt: Date.now() });
    setRoute('recording');
  };

  // End & upload — write file, generate digest, composite /calls upload, record history.
  const endCall = async (transcriptText: string) => {
    const call = activeCall;
    if (!call) return;
    const capture: CaptureResult = { ...call.meta, transcriptText };
    const outcome = await finalizeCall({ digest, config, uploadQueue, history }, capture);
    await refreshRecords();
    setActiveCall(null);
    setCallStarted(false);
    setSelectedId(outcome.record.id);
    // Folder problem at save time: nothing is lost (uploaded/queued + text retained on the
    // record), but tell the user clearly and point them to Settings to re-select a folder.
    setFileWarning(
      outcome.fileSaved
        ? null
        : `Your transcript was ${outcome.status === 'done' ? 'uploaded' : 'saved for upload'} and is safe, but the local copy couldn’t be written: ${outcome.fileError} Choose a writable folder in Settings — the next capture will save there.`,
    );
    if (outcome.status === 'reauth') {
      setStage('login');
      return;
    }
    if (outcome.status === 'deEntitled') {
      reGate();
      return;
    }
    setRoute('transcripts');
  };

  // Discard the in-progress call: the native capture is already stopped by the Recording screen.
  // Drop it WITHOUT finalizing/uploading and WITHOUT a history entry — the user explicitly
  // confirmed they want it deleted — and return to the dashboard.
  const discardCall = () => {
    setActiveCall(null);
    setCallStarted(false);
    setSelectedId(null);
    setGuardError(null);
    setFileWarning(null);
    setRoute('dashboard');
  };

  const openTranscript = (id: string) => {
    setSelectedId(id);
    setRoute('transcripts');
  };

  // Delete a transcript from LOCAL history + its stored file. Never touches the ValueOS cloud
  // copy (an already-uploaded transcript stays in ValueOS).
  const deleteTranscript = async (id: string) => {
    const rec = records.find((r) => r.id === id);
    await history.remove(id);
    if (rec?.path) {
      try {
        await config.deleteTranscriptFile(rec.path);
      } catch {
        /* file already gone / unwritable — the history entry is removed regardless */
      }
    }
    await refreshRecords();
    setSelectedId((cur) => (cur === id ? null : cur));
  };

  const navigate = (r: MainRoute) => {
    setGuardError(null);
    setFileWarning(null);
    setRoute(r);
  };

  // ── onboarding stages (full-bleed blue, no shell) ─────────────────────────
  if (stage === 'welcome') return <WelcomeScreen onProceed={() => setStage('login')} />;
  if (stage === 'login') return <LoginScreen onDone={handleLogin} />;
  if (stage === 'blocked')
    return <BlockedScreen reason={blockedReason} onContact={contactSales} onRetry={() => setStage('login')} />;
  if (stage === 'setup') return <SetupScreen onComplete={afterSetup} />;
  if (stage === 'storage') return <StorageScreen onDone={() => setStage('main')} />;

  // ── main app (shell) ──────────────────────────────────────────────────────
  return (
    <>
    <AppShell route={route} onNavigate={navigate} onReportBug={() => setBugOpen(true)}>
      {guardError && (
        <div
          data-testid="valueos-guard-error"
          role="alert"
          style={{
            margin: '20px 32px 0',
            padding: '12px 16px',
            borderRadius: 10,
            background: 'rgba(206,54,68,.08)',
            color: 'var(--va-signal-red)',
            border: '1px solid rgba(206,54,68,.25)',
            fontSize: 14,
            display: 'flex',
            justifyContent: 'space-between',
            gap: 12,
            alignItems: 'center',
          }}
        >
          <span>{guardError}</span>
          <button
            className="va-btn va-btn-danger va-btn-sm"
            onClick={() => {
              setGuardError(null);
              setRoute('recording');
            }}
          >
            Go to recording
          </button>
        </div>
      )}

      {fileWarning && (
        <div
          data-testid="valueos-file-warning"
          role="alert"
          style={{
            margin: '20px 32px 0',
            padding: '12px 16px',
            borderRadius: 10,
            background: 'rgba(206,54,68,.06)',
            color: 'var(--va-signal-red)',
            border: '1px solid rgba(206,54,68,.22)',
            fontSize: 14,
            display: 'flex',
            justifyContent: 'space-between',
            gap: 12,
            alignItems: 'center',
          }}
        >
          <span>{fileWarning}</span>
          <button className="va-btn va-btn-danger-outline va-btn-sm" onClick={() => navigate('settings')}>
            Open Settings
          </button>
        </div>
      )}

      {route === 'dashboard' && (
        <Dashboard
          records={records}
          activeCall={activeCall}
          onNew={requestNew}
          onOpenRecording={() => setRoute('recording')}
          onOpenTranscript={openTranscript}
        />
      )}

      {route === 'transcripts' && (
        <Transcripts
          records={records}
          activeCall={activeCall}
          selectedId={selectedId}
          onSelect={setSelectedId}
          onNew={requestNew}
          onOpenRecording={() => setRoute('recording')}
          onDelete={deleteTranscript}
        />
      )}

      {route === 'settings' && (
        <Settings onLogout={logout} tenantId={entitled[0]?.tenant.id} onReportBug={() => setBugOpen(true)} />
      )}

      {route === 'recording' &&
        (activeCall ? (
          <Recording
            key={activeCall.startedAt}
            meta={activeCall.meta}
            startedAt={activeCall.startedAt}
            hasStarted={callStarted}
            onStarted={() => setCallStarted(true)}
            onEnd={endCall}
            onDiscard={discardCall}
          />
        ) : (
          <NoActiveCall onNew={requestNew} />
        ))}

      {wizardOpen && (
        <Wizard
          entitledTenants={entitled}
          onClose={() => setWizardOpen(false)}
          onStart={startCall}
          onLostAccess={() => {
            setWizardOpen(false);
            reGate();
          }}
        />
      )}
    </AppShell>
    {bugOpen && <BugReportDialog onClose={() => setBugOpen(false)} tenantId={entitled[0]?.tenant.id} />}
    </>
  );
}

function NoActiveCall({ onNew }: { onNew: () => void }) {
  return (
    <div className="va-page" data-testid="valueos-no-active-call">
      <h1 style={{ fontFamily: 'var(--font-display)', fontWeight: 800, fontSize: 28 }}>No call in progress</h1>
      <p className="va-body" style={{ marginTop: 8 }}>Start a new transcript to begin recording.</p>
      <button className="va-btn va-btn-primary" style={{ marginTop: 16 }} onClick={onNew}>New transcript</button>
    </div>
  );
}
