// VALUEOS: the finalize step — write the transcript file, generate the high-level digest, and
// upload the call + transcript + digest in ONE atomic op (composite /calls) via the resilient
// pending-upload queue, recording it in local history. Split into two phases so the UI does NOT
// have to block on the network:
//   • enqueueCall()      — fast: digest + file + enqueue + record history as "pending". Returns.
//   • flushCallUploads() — background: drain the queue and reconcile each record's status.
// finalizeCall() runs both in sequence (the synchronous behaviour the tests assert). The
// recording screen uses the two phases directly: enqueue → navigate away immediately → flush in
// the background. Idempotency key is stable across retries → no duplicates.
import type { ValueOsServices } from '../context/ValueOsProvider';
import type { CreateCallRequest } from '../api/types';
import type { CaptureResult } from '../shell/flowTypes';
import type { TranscriptRecord } from '../history/transcriptHistory';

export type FinalizeStatus = 'done' | 'pending' | 'reauth' | 'deEntitled' | 'error';

export interface FinalizeOutcome {
  status: FinalizeStatus;
  detail?: string;
  /** The history record written for this call (always written, so nothing is ever lost). */
  record: TranscriptRecord;
  /** Whether the local .txt was written to the configured folder. When false, the upload
   *  still proceeded and the full text is retained on the record (`record.transcript`) and in
   *  the upload body — nothing is lost — but the user should pick a writable folder. */
  fileSaved: boolean;
  /** The write error, when `fileSaved` is false (e.g. folder missing/unwritable). */
  fileError?: string;
}

/** The result of enqueueCall — the "pending" record plus the local-file outcome. */
export interface EnqueueResult {
  record: TranscriptRecord;
  fileSaved: boolean;
  fileError?: string;
}

function sanitize(s: string): string {
  return s.replace(/[^a-z0-9]+/gi, '-').toLowerCase().slice(0, 40) || 'meeting';
}

export function newIdempotencyKey(): string {
  return globalThis.crypto?.randomUUID?.() ?? `id-${Math.random().toString(36).slice(2)}`;
}

type EnqueueServices = Pick<ValueOsServices, 'digest' | 'config' | 'uploadQueue' | 'history'>;
type FlushServices = Pick<ValueOsServices, 'uploadQueue' | 'history'>;

/**
 * PHASE 1 (fast): generate the digest, write the local .txt, enqueue the composite /calls upload,
 * and record the call in local history as "pending" — then return WITHOUT touching the network.
 * A missing/unwritable folder must NOT drop the transcript: the full text still rides in the
 * upload body (raw_content) and on the history record, so we continue regardless and report the
 * folder problem via `fileSaved`/`fileError`.
 */
export async function enqueueCall(
  services: EnqueueServices,
  capture: CaptureResult,
  idempotencyKey: string = newIdempotencyKey(),
  now: () => number = () => Date.now(),
): Promise<EnqueueResult> {
  const { digest, config, uploadQueue, history } = services;
  const key = idempotencyKey;

  const digestText = await digest.generate(capture.transcriptText, { title: capture.callName });
  const fileName = `${sanitize(capture.callName)}-${key.slice(0, 8)}.txt`;
  let path = '';
  let fileError: string | undefined;
  try {
    path = await config.writeTranscriptFile(fileName, capture.transcriptText);
  } catch (e) {
    fileError = (e as Error)?.message ?? 'Could not write the transcript file to the configured folder.';
  }

  // NESTED transcript object (VALUEOS_AGENT_API.md §4 — do NOT flatten).
  const request: CreateCallRequest = {
    name: capture.callName,
    ...(capture.activityType === 'lead'
      ? { lead_id: capture.targetId }
      : { opportunity_id: capture.targetId }),
    transcript: {
      raw_content: capture.transcriptText,
      digest: digestText,
      file_name: fileName,
      title: capture.callName,
    },
    idempotency_key: key,
  };

  await uploadQueue.enqueue({ id: key, tenantId: capture.tenantId, request, transcriptPath: path });

  const record: TranscriptRecord = {
    id: key,
    targetLabel: capture.targetLabel,
    tenantId: capture.tenantId,
    tenantName: capture.tenantName,
    activityType: capture.activityType,
    targetId: capture.targetId,
    createdAt: now(),
    path,
    uploadStatus: 'pending', // reconciled by flushCallUploads once the upload resolves
    digest: digestText,
    transcript: capture.transcriptText,
  };
  await history.add(record);

  return { record, fileSaved: !fileError, fileError };
}

/**
 * PHASE 2 (background): drain the pending-upload queue and reconcile each affected history record's
 * status (uploaded / failed / still-pending). Safe to call any time (e.g. on app start) to retry
 * everything queued. Returns the overall status for the just-finished flush so the caller can react
 * to reauth / de-entitlement.
 */
export async function flushCallUploads(services: FlushServices): Promise<FinalizeOutcome['status']> {
  const { uploadQueue, history } = services;
  const outcome = await uploadQueue.flush();
  const records = await history.list();
  const setStatus = async (id: string, uploadStatus: TranscriptRecord['uploadStatus'], error?: string) => {
    const r = records.find((x) => x.id === id);
    if (r) await history.add({ ...r, uploadStatus, error });
  };

  for (const id of outcome.uploaded) await setStatus(id, 'uploaded', undefined);
  for (const f of outcome.failed) {
    await setStatus(f.id, 'failed', `${f.status ? f.status + ' · ' : ''}${f.message}`);
  }
  for (const tid of outcome.deEntitled) {
    for (const r of records.filter((x) => x.tenantId === tid && x.uploadStatus !== 'uploaded')) {
      await setStatus(r.id, 'failed', 'This workspace no longer has ValueOS Agent access.');
    }
  }
  // Retained ids stay 'pending' (a later flush retries them).

  if (outcome.needsReauth) return 'reauth';
  if (outcome.deEntitled.length) return 'deEntitled';
  if (outcome.failed.length) return 'error';
  if (outcome.retained.length) return 'pending';
  return 'done';
}

/**
 * Synchronous finalize (enqueue → flush), returning the fully-reconciled outcome. The recording
 * screen no longer uses this directly (it decouples the two phases so the UI isn't blocked), but
 * it is kept for callers/tests that want the whole thing to complete before returning.
 */
export async function finalizeCall(
  services: EnqueueServices,
  capture: CaptureResult,
  idempotencyKey: string = newIdempotencyKey(),
  now: () => number = () => Date.now(),
): Promise<FinalizeOutcome> {
  const { record, fileSaved, fileError } = await enqueueCall(services, capture, idempotencyKey, now);
  const status = await flushCallUploads(services);
  const finalRecord = (await services.history.list()).find((r) => r.id === record.id) ?? record;
  const detail =
    status === 'error'
      ? `Upload failed: ${finalRecord.error ?? 'not sent'}`
      : status === 'pending'
        ? 'Saved locally — will retry the upload.'
        : undefined;
  return { status, detail, record: finalRecord, fileSaved, fileError };
}
