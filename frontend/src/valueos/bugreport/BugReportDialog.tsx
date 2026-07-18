'use client';
// VALUEOS WS3: the "Report a bug" dialog. Free-text description + a visible note of exactly
// what will be attached (logs + metadata) for consent. On submit: build → SCRUB → send. On
// failure, offer to save the bundle locally so it isn't lost. The bundle is always scrubbed
// (fail-closed) before it can leave the machine.
import React, { useRef, useState } from 'react';
import { useValueOs } from '../context/ValueOsProvider';
import { buildBugReport, newIdempotencyKey } from './buildBugReport';
import { realMetadataSource } from './metadata';
import { getRecentLogs } from './logBuffer';
import { saveBugReportLocally } from './service';
import type { BugReportBundle } from './types';
import { IcClose } from '../app/icons';

type Phase = 'form' | 'submitting' | 'success' | 'error';

export function BugReportDialog({ onClose, tenantId }: { onClose: () => void; tenantId?: string }) {
  const { bugReport } = useValueOs();
  const [phase, setPhase] = useState<Phase>('form');
  const [description, setDescription] = useState('');
  const [message, setMessage] = useState('');
  const idemRef = useRef<string>('');
  const bundleRef = useRef<BugReportBundle | null>(null);

  const submit = async () => {
    setPhase('submitting');
    setMessage('');
    try {
      if (!idemRef.current) idemRef.current = newIdempotencyKey();
      const bundle = await buildBugReport({
        description: description.trim() || '(no description)',
        tenantId,
        meta: realMetadataSource,
        logs: getRecentLogs(),
        idempotencyKey: idemRef.current, // stable across retries
      });
      bundleRef.current = bundle;
      const res = await bugReport.submit(bundle);
      setPhase('success');
      setMessage(
        res.savedLocally
          ? `Saved on this device${res.localPath ? ` (${res.localPath})` : ''}. It will be sent to Value Accelerator once reporting is enabled.`
          : `Thanks — your report was sent. Reference: ${res.reportId}.`,
      );
    } catch (e) {
      setPhase('error');
      setMessage((e as Error)?.message ?? 'Could not send the report.');
    }
  };

  const saveLocally = async () => {
    if (!bundleRef.current) return;
    const path = await saveBugReportLocally(bundleRef.current);
    setPhase('success');
    setMessage(path ? `Saved on this device: ${path}` : 'Saved on this device.');
  };

  return (
    <div className="va-scrim va-root" data-testid="valueos-bugreport" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div className="va-modal" role="dialog" aria-modal="true">
        <div className="va-modal-head">
          <div className="va-ovl">Report a bug</div>
          <button className="va-btn va-btn-ghost-light va-btn-sm" data-testid="valueos-bugreport-close" onClick={onClose} aria-label="Close" style={{ padding: 8, borderRadius: 8 }}>
            <IcClose size={16} />
          </button>
        </div>

        <div className="va-modal-body va-scroll" style={{ minHeight: 200 }}>
          {phase === 'success' ? (
            <div data-testid="valueos-bugreport-success" style={{ padding: '12px 0' }}>
              <h3 style={{ fontFamily: 'var(--font-display)', fontWeight: 800, fontSize: 20, margin: '0 0 8px' }}>Report sent</h3>
              <p className="va-body" style={{ margin: 0 }}>{message}</p>
            </div>
          ) : (
            <>
              <label className="va-ovl" htmlFor="bug-desc">What went wrong?</label>
              <textarea
                id="bug-desc"
                data-testid="valueos-bugreport-desc"
                className="va-input"
                style={{ minHeight: 110, resize: 'vertical', marginTop: 6 }}
                placeholder="Describe what happened, e.g. “transcript didn’t upload after I ended the call.”"
                value={description}
                onChange={(e) => setDescription(e.target.value)}
              />

              <div className="va-card" style={{ marginTop: 14, padding: 14, boxShadow: 'none' }}>
                <div className="va-ovl" style={{ marginBottom: 8 }}>What we’ll attach</div>
                <ul className="va-body" style={{ margin: 0, paddingLeft: 18, fontSize: 13.5, lineHeight: 1.7 }}>
                  <li>Your description above</li>
                  <li>Recent app logs (scrubbed — no tokens, emails, or transcript text)</li>
                  <li>App version &amp; build, OS/platform &amp; architecture, install id, current workspace, time zone</li>
                </ul>
                <p className="va-muted" style={{ fontSize: 12.5, margin: '10px 0 0' }}>
                  Your recordings and transcript text are <strong>never</strong> attached.
                </p>
              </div>

              {phase === 'error' && (
                <p data-testid="valueos-bugreport-error" style={{ color: 'var(--va-signal-red)', fontSize: 13.5, margin: '12px 0 0' }}>
                  {message} You can save it on this device instead so it isn’t lost.
                </p>
              )}
            </>
          )}
        </div>

        <div className="va-modal-foot">
          <button className="va-btn va-btn-ghost-light" onClick={onClose}>
            {phase === 'success' ? 'Done' : 'Cancel'}
          </button>
          {phase === 'form' && (
            <button className="va-btn va-btn-primary" data-testid="valueos-bugreport-submit" onClick={submit}>
              Send report
            </button>
          )}
          {phase === 'submitting' && (
            <button className="va-btn va-btn-primary" disabled>Sending…</button>
          )}
          {phase === 'error' && (
            <div style={{ display: 'flex', gap: 8 }}>
              <button className="va-btn va-btn-ghost-light" data-testid="valueos-bugreport-savelocal" onClick={saveLocally}>Save locally</button>
              <button className="va-btn va-btn-primary" data-testid="valueos-bugreport-retry" onClick={submit}>Try again</button>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
