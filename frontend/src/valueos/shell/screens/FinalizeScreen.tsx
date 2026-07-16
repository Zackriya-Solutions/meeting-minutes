import React, { useEffect, useRef, useState } from 'react';
import { useValueOs } from '../../context/ValueOsProvider';
import type { CaptureResult } from '../flowTypes';
import * as ui from './ui';

// VALUEOS: finalize — store the transcript locally, generate the high-level digest, and
// upload BOTH (transcript + digest) to the pre-selected target with a client idempotency
// key, via the resilient pending-upload queue. 401 → re-auth; network/503 → kept for retry;
// 403 feat_agent → the workspace lost access, re-gate (§2.7); terminal (404/413/422) → failed.
type Status = 'working' | 'done' | 'pending' | 'reauth' | 'deEntitled' | 'error';

function sanitize(s: string): string {
  return s.replace(/[^a-z0-9]+/gi, '-').toLowerCase().slice(0, 40) || 'meeting';
}
function newIdempotencyKey(): string {
  return globalThis.crypto?.randomUUID?.() ?? `id-${Math.random().toString(36).slice(2)}`;
}

export function FinalizeScreen({
  capture,
  onDone,
  onReauth,
  onLostAccess,
}: {
  capture: CaptureResult;
  onDone: () => void;
  onReauth: () => void;
  onLostAccess: () => void;
}) {
  const { digest, config, uploadQueue, history } = useValueOs();
  const [status, setStatus] = useState<Status>('working');
  const [detail, setDetail] = useState('');
  const idemRef = useRef<string>('');
  const startedRef = useRef(false);

  const run = async () => {
    setStatus('working');
    try {
      if (!idemRef.current) idemRef.current = newIdempotencyKey();
      const key = idemRef.current; // stable across retries → idempotent, no duplicates
      const digestText = await digest.generate(capture.transcriptText, {
        title: capture.targetLabel,
      });
      const fileName = `${sanitize(capture.targetLabel)}-${key.slice(0, 8)}.txt`;
      const path = await config.writeTranscriptFile(fileName, capture.transcriptText);
      await uploadQueue.enqueue({
        id: key,
        tenantId: capture.tenantId,
        activityType: capture.activityType,
        targetId: capture.targetId,
        transcriptPath: path,
        request: {
          raw_content: capture.transcriptText, // artifact 1: transcript
          digest: digestText, // artifact 2: high-level recap
          idempotency_key: key,
          file_name: fileName,
          title: capture.targetLabel,
        },
      });
      const outcome = await uploadQueue.flush();
      const uploaded = outcome.uploaded.includes(key);
      const failed = outcome.failed.find((f) => f.id === key);
      const lostAccess = outcome.deEntitled.includes(capture.tenantId);
      // Record in the local transcript history (shown on the home screen). The transcript
      // file is on disk regardless, so nothing is lost even on a terminal failure.
      await history.add({
        id: key,
        targetLabel: capture.targetLabel,
        tenantId: capture.tenantId,
        tenantName: capture.tenantName,
        activityType: capture.activityType,
        targetId: capture.targetId,
        createdAt: Date.now(),
        path,
        uploadStatus: uploaded ? 'uploaded' : failed || lostAccess ? 'failed' : 'pending',
        digest: digestText,
        transcript: capture.transcriptText,
      });
      if (outcome.needsReauth) setStatus('reauth');
      else if (lostAccess) setStatus('deEntitled');
      else if (failed) {
        setStatus('error');
        setDetail(`Upload failed: ${failed.message}`);
      } else if (outcome.retained.includes(key)) {
        setStatus('pending');
        setDetail('Saved locally — will retry the upload.');
      } else setStatus('done');
    } catch (e) {
      setStatus('error');
      setDetail((e as Error)?.message ?? 'Upload failed');
    }
  };

  useEffect(() => {
    if (!startedRef.current) {
      startedRef.current = true;
      void run();
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const heading =
    status === 'done'
      ? 'Uploaded'
      : status === 'working'
        ? 'Finishing up…'
        : status === 'pending'
          ? 'Saved — upload pending'
          : status === 'reauth'
            ? 'Please sign in again'
            : status === 'deEntitled'
              ? 'Workspace access changed'
              : 'Something went wrong';

  return (
    <div data-testid="valueos-finalize" style={ui.page}>
      <div style={ui.card}>
        <h1 style={ui.h1}>{heading}</h1>
        <p data-testid="valueos-finalize-status" style={ui.sub}>
          {status === 'working' && 'Generating the recap and uploading the transcript and digest…'}
          {status === 'done' && `Transcript and recap attached to ${capture.targetLabel}.`}
          {status === 'pending' && (detail || 'Your transcript is safe locally and will upload when possible.')}
          {status === 'reauth' && 'Your session expired. Sign in again to finish the upload — your transcript is saved.'}
          {status === 'deEntitled' &&
            'This workspace no longer has ValueOS Agent access, so the upload was stopped. Your transcript is saved locally.'}
          {status === 'error' && detail}
        </p>
        {status === 'done' && (
          <button data-testid="valueos-finalize-done" style={ui.primaryBtn} onClick={onDone}>
            Capture another
          </button>
        )}
        {status === 'pending' && (
          <button data-testid="valueos-finalize-retry" style={ui.primaryBtn} onClick={run}>
            Retry now
          </button>
        )}
        {status === 'reauth' && (
          <button data-testid="valueos-finalize-reauth" style={ui.primaryBtn} onClick={onReauth}>
            Sign in
          </button>
        )}
        {status === 'deEntitled' && (
          <button
            data-testid="valueos-finalize-deentitled"
            style={ui.primaryBtn}
            onClick={onLostAccess}
          >
            Continue
          </button>
        )}
        {status === 'error' && (
          <button data-testid="valueos-finalize-retry" style={ui.ghostBtn} onClick={run}>
            Try again
          </button>
        )}
      </div>
      <footer style={ui.footer}>Value Accelerator GmbH</footer>
    </div>
  );
}
